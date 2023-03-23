mod headers;

use axum::{
    error_handling::HandleErrorLayer,
    extract::{BodyStream, State},
    headers::{authorization::Bearer, Authorization},
    http::{Request, StatusCode},
    routing::post,
    Json, Router, TypedHeader,
};
use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use headers::{GithubEvent, GithubSignature256};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use tokio::{process::Command, signal};
use tower::{BoxError, ServiceBuilder};
use tower_governor::{
    errors::display_error, governor::GovernorConfigBuilder, key_extractor::KeyExtractor,
    GovernorError, GovernorLayer,
};

#[derive(Parser)]
struct Opt {
    #[clap(short, long, default_value_t = 5000)]
    port: u16,

    #[clap(subcommand)]
    command: Option<TokenCommand>,
}

#[derive(Subcommand, Clone, Eq, PartialEq)]
enum TokenCommand {
    Github { secret: String, events: Vec<String> },
    Token { bearer: String },
}

#[derive(Clone)]
struct Token(Option<TokenCommand>);

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let opt = Opt::parse();

    match opt.command.as_ref() {
        Some(TokenCommand::Token { .. }) => {
            tracing::info!("accepting authorization header");
        }
        Some(TokenCommand::Github { events, .. }) => {
            tracing::info!("accepting github events: {:?}", events);
        }
        _ => {}
    }

    let governor_conf = Box::new(
        GovernorConfigBuilder::default()
            .per_second(10)
            .burst_size(5)
            .key_extractor(UserToken)
            .use_headers()
            .finish()
            .unwrap(),
    );

    // build our application with a route
    let app = Router::new()
        .route("/hook", post(handler))
        .with_state(Token(opt.command))
        .layer(
            ServiceBuilder::new()
                // this middleware goes above `GovernorLayer` because it will receive
                // errors returned by `GovernorLayer`
                .layer(HandleErrorLayer::new(|e: BoxError| async move {
                    display_error(e)
                }))
                .layer(GovernorLayer {
                    config: Box::leak(governor_conf),
                }),
        );

    // run it
    let addr = SocketAddr::from(([0, 0, 0, 0], opt.port));
    tracing::info!("listening on {}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn handler(
    State(Token(token)): State<Token>,
    auth: Option<TypedHeader<Authorization<Bearer>>>,
    github_signature: Option<TypedHeader<GithubSignature256>>,
    github_event: Option<TypedHeader<GithubEvent>>,
    mut stream: BodyStream,
) -> Result<Json<Vec<AutoUpdateReponse>>, (StatusCode, ())> {
    match (token, auth, github_signature, github_event) {
        (Some(TokenCommand::Token { bearer: t1 }), Some(TypedHeader(t2)), None, None)
            if t1 == t2.token() => {}
        (Some(TokenCommand::Token { .. }), _, _, _) => {
            tracing::debug!("token mismatch");
            return Err((StatusCode::UNAUTHORIZED, ()));
        }
        (
            Some(TokenCommand::Github { secret, events }),
            None,
            Some(TypedHeader(GithubSignature256(signature))),
            event,
        ) => {
            let mut hasher = Sha256::new();
            hasher.update(secret);
            while let Some(Ok(b)) = stream.next().await {
                hasher.update(b);
            }

            let (_, signature_exp) = signature
                .split_once('=')
                .ok_or((StatusCode::BAD_REQUEST, ()))?;

            let signature = hex::encode(hasher.finalize());

            if signature != signature_exp {
                tracing::debug!("github signature mismatch");
                return Err((StatusCode::UNAUTHORIZED, ()));
            }

            match (&events[..], event) {
                ([], _) => {}
                (_, None) => {
                    tracing::debug!("missing github event header");
                    return Err((StatusCode::BAD_REQUEST, ()));
                }
                (e, Some(TypedHeader(GithubEvent(event)))) if !e.contains(&event) => {
                    tracing::debug!("github event mismatch, ignoring");
                    return Err((StatusCode::OK, ()));
                }
                _ => {}
            }
        }
        (Some(TokenCommand::Github { .. }), _, None, _) => {
            tracing::debug!("missing github signature header");
            return Err((StatusCode::BAD_REQUEST, ()));
        }
        _ => {}
    }

    tracing::info!("running update");

    let command = match Command::new("podman")
        .arg("auto-update")
        .arg("--format")
        .arg("json")
        .output()
        .await
    {
        Ok(c) if c.status.success() => c,
        Err(e) => {
            tracing::error!("failed to run command: {}", e);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, ()));
        }
        Ok(c) => {
            tracing::error!(
                "command failed with status {}: {}",
                c.status,
                String::from_utf8_lossy(&c.stderr)
            );
            return Err((StatusCode::INTERNAL_SERVER_ERROR, ()));
        }
    };

    tracing::debug!("stdout: {}", String::from_utf8_lossy(&command.stdout));
    if !command.stderr.is_empty() {
        tracing::error!("stderr: {}", String::from_utf8_lossy(&command.stderr));
    }

    let response: Vec<AutoUpdateReponse> = if command.stdout.starts_with("[".as_bytes()) {
        serde_json::from_slice(&command.stdout).expect("failed to parse")
    } else {
        vec![]
    };

    Ok(Json(response))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AutoUpdateReponse {
    unit: String,
    container: String,
    image: String,
    container_name: String,
    #[serde(rename = "ContainerID")]
    container_id: String,
    policy: String,
    updated: Updated,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Updated {
    False,
    Pending,
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    println!("signal received, starting graceful shutdown");
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
struct UserToken;

impl KeyExtractor for UserToken {
    type Key = String;
    type KeyExtractionError = GovernorError;

    fn extract<B>(&self, req: &Request<B>) -> Result<Self::Key, Self::KeyExtractionError> {
        Ok(req
            .headers()
            .get("Authorization")
            .and_then(|token| token.to_str().ok())
            .and_then(|token| token.strip_prefix("Bearer "))
            .map(|token| token.trim().to_owned())
            .unwrap_or_default())
    }

    fn key_name(&self, key: &Self::Key) -> Option<String> {
        Some(key.clone())
    }

    fn name(&self) -> &'static str {
        "UserToken"
    }
}
