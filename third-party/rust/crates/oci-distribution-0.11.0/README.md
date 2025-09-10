# OCI Distribution

[![oci-distribution documentation](https://docs.rs/oci-distribution/badge.svg)](https://docs.rs/oci-distribution)

> HEADS UP! If you have contributed to this repository or already have cloned the git repo, we recently
> cleaned out our git history of some large blobs. However, this means we mucked with history. Please
> reclone the repository to avoid any problems when contributing. See #18 for more details

This Rust library implements the
[OCI Distribution specification](https://github.com/opencontainers/distribution-spec/blob/master/spec.md),
which is the protocol that Docker Hub and other container registries use.

The immediate goal of this crate is to provide a way to pull WASM modules from
a Docker registry. However, our broader goal is to implement the spec in its
entirety.

## Community, discussion, contribution, and support

You can reach the Krustlet community and developers via the following channels:

- [Kubernetes Slack](https://kubernetes.slack.com):
  - [#krustlet](https://kubernetes.slack.com/messages/krustlet)
- Public Community Call on Mondays at 1:00 PM PT:
  - [Zoom](https://us04web.zoom.us/j/71695031152?pwd=T0g1d0JDZVdiMHpNNVF1blhxVC9qUT09)
  - Download the meeting calendar invite
    [here](./community_meeting.ics)

## Code of Conduct

This project has adopted the [CNCF Code of
Conduct](https://github.com/cncf/foundation/blob/master/code-of-conduct.md).
