use axum::{
    headers::{Error, Header, HeaderName},
    http::HeaderValue,
};

pub struct GithubSignature256(pub String);

impl Header for GithubSignature256 {
    fn name() -> &'static axum::headers::HeaderName {
        static SIGNATURE_HEADER: HeaderName = HeaderName::from_static("x-hub-signature-256");
        &SIGNATURE_HEADER
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i HeaderValue>,
    {
        values
            .next()
            .map(|v| {
                let v = v.to_str().map_err(|_| Error::invalid())?;
                Ok(GithubSignature256(v.to_string()))
            })
            .unwrap_or(Err(Error::invalid()))
    }

    fn encode<E: Extend<HeaderValue>>(&self, _values: &mut E) {
        unimplemented!()
    }
}

pub struct GithubEvent(pub String);

impl Header for GithubEvent {
    fn name() -> &'static axum::headers::HeaderName {
        static SIGNATURE_HEADER: HeaderName = HeaderName::from_static("x-github-event");
        &SIGNATURE_HEADER
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i HeaderValue>,
    {
        values
            .next()
            .map(|v| {
                let v = v.to_str().map_err(|_| Error::invalid())?;
                Ok(GithubEvent(v.to_string()))
            })
            .unwrap_or(Err(Error::invalid()))
    }

    fn encode<E: Extend<HeaderValue>>(&self, _values: &mut E) {
        unimplemented!()
    }
}
