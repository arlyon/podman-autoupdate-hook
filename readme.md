# podman-autoupdate-hook

This is a handy little server for running podman containers with auto-update. It allows you to set up webhooks to alert your server when a new image is available, and then automatically update the container to use the new image.

## Installation

This can be installed quite simply using cargo. You can install it with:

```bash
cargo install podman-autoupdate-hook
```

## Set up

There are currently two optional flags, the port and a bearer token. The server will listen on all interfaces.

```bash
podman-autoupdate-hook --port 8080 --token my_secret
http localhost:8080 'Authorization: Bearer my_secret'
```

Upon receiving this request, podman will attempt to pull a new version for all containers with the label `io.containers.autoupdate`. If a new version is available, it will be pulled and the container will be restarted, and will automatically roll back if the new version fails to start.

These containers are expected to be running using systemd with an appropriate unit file. For more information, see here: https://docs.podman.io/en/latest/markdown/podman-auto-update.1.html#description

## Usage

Github actions is a good way to set up a webhook. You can use the following action to set up a webhook to your server:

```yaml
name: release
on: push
jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
        #
        # build the container and push to registry
        #
      - run: |
          curl myserver.com:8080/hook -H 'Authorization: Bearer my_secret'
```
