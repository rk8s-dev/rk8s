## Distribution

A lite implement of OCI Distribution Spec in rust.

## Quick Start

Start registry:

```
distribution
```

More options:

```
Usage: distribution [OPTIONS]

Options:
      --host <HOST>        Registry listening host [default: 127.0.0.1]
  -p, --port <PORT>        Registry listening port [default: 8968]
  -s, --storage <STORAGE>  Storage backend type [default: FILESYSTEM]
      --root <ROOT>        Registry root path [default: /var/lib/oci-registry]
  -h, --help               Print help
  -V, --version            Print version
```

## Build from source

Build with Buck2 (make sure you have followed the workflow in `third-party/README.md` before your build):

```
buck2 build //project/distribution:distribution
```

Another option is to build with Cargo:

```
cd project/distribution/
cargo build
```

## Compatibility

The distribution registry implements the [OCI Distribution Spec](https://github.com/opencontainers/distribution-spec) version 1.1.1.

| ID      | Method         | API Endpoint                                                 | Compatibility |
| ------- | -------------- | ------------------------------------------------------------ | ------------- |
| end-1   | `GET`          | `/v2/`                                                       | ✅             |
| end-2   | `GET` / `HEAD` | `/v2/<name>/blobs/<digest>`                                  | ✅             |
| end-3   | `GET` / `HEAD` | `/v2/<name>/manifests/<reference>`                           | ✅             |
| end-4a  | `POST`         | `/v2/<name>/blobs/uploads/`                                  | ✅             |
| end-4b  | `POST`         | `/v2/<name>/blobs/uploads/?digest=<digest>`                  | ✅             |
| end-5   | `PATCH`        | `/v2/<name>/blobs/uploads/<reference>`                       | ✅             |
| end-6   | `PUT`          | `/v2/<name>/blobs/uploads/<reference>?digest=<digest>`       | ✅             |
| end-7   | `PUT`          | `/v2/<name>/manifests/<reference>`                           | ✅             |
| end-8a  | `GET`          | `/v2/<name>/tags/list`                                       | ✅             |
| end-8b  | `GET`          | `/v2/<name>/tags/list?n=<integer>&last=<tagname>`            | ✅             |
| end-9   | `DELETE`       | `/v2/<name>/manifests/<reference>`                           | ✅             |
| end-10  | `DELETE`       | `/v2/<name>/blobs/<digest>`                                  | ✅             |
| end-11  | `POST`         | `/v2/<name>/blobs/uploads/?mount=<digest>&from=<other_name>` | 🚧             |
| end-12a | `GET`          | `/v2/<name>/referrers/<digest>`                              | 🚧             |
| end-12b | `GET`          | `/v2/<name>/referrers/<digest>?artifactType=<artifactType>`  | 🚧             |
| end-13  | `GET`          | `/v2/<name>/blobs/uploads/<reference>`                       | ✅             |

## Conformance Tests

To run the conformance tests provided by OCI Distribution Spec, you need to install Go 1.17+ first, and then clone the distribution-spec repository:

```
git clone git@github.com:opencontainers/distribution-spec.git
```

In the `conformance` directory, apply a patch and build the test binary:

```
cd distribution-spec/conformance/
go test -c
```

This will produce an executable at `conformance.test`.

Next, set environment variables with the registry details:

```
# Registry details
export OCI_ROOT_URL="http://127.0.0.1:8968"
export OCI_NAMESPACE="myorg/myrepo"
export OCI_CROSSMOUNT_NAMESPACE="myorg/other"
export OCI_USERNAME="myuser"
export OCI_PASSWORD="mypass"

# Which workflows to run
export OCI_TEST_PULL=1
export OCI_TEST_PUSH=1
export OCI_TEST_CONTENT_DISCOVERY=1
export OCI_TEST_CONTENT_MANAGEMENT=1

# Extra settings
export OCI_HIDE_SKIPPED_WORKFLOWS=0
export OCI_DEBUG=0
export OCI_DELETE_MANIFEST_BEFORE_BLOBS=0 # defaults to OCI_DELETE_MANIFEST_BEFORE_BLOBS=1 if not set
```

Lastly, run the tests:

```
./conformance.test
```

