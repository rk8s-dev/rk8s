# Distribution

A lite implementation of the OCI Distribution Spec in Rust, with added support for user management and authentication.

## Configuration

The registry is configured via command-line arguments or an environment file. Using a `.env` file in the project's root directory is the recommended approach, especially for managing sensitive values.

Create a `.env` file with the following content:

```dotenv
# Application Host and Port
OCI_REGISTRY_URL=127.0.0.1
OCI_REGISTRY_PORT=8968

# Public URL used in responses
OCI_REGISTRY_PUBLIC_URL=http://127.0.0.1:8968

# Storage Configuration
OCI_REGISTRY_STORAGE=FILESYSTEM
OCI_REGISTRY_ROOTDIR=/var/lib/registry

# Database URL for storing user and repository metadata
DATABASE_URL="postgres://postgres:password@localhost:5432/postgres"

# --- Security Configuration ---
# A random, secret string used for salting passwords with 16 characters long.
# Generate a secure random string for production use.
PASSWORD_SALT="AAAAAAAAAAAAAAAA"

# A secret key for signing JWT tokens.
# Generate a secure random string for production use.
JWT_SECRET="secret"

# JWT token lifetime in seconds
JWT_LIFETIME_SECONDS=3600

# Log level
RUST_LOG="info"
```

**Important**: For production environments, `PASSWORD_SALT` and `JWT_SECRET` must be protected.

## Quick Start

1.  **Configure the registry**: Create a `.env` file in the root of the project as described in the Configuration section.
2.  **Start the registry**: Run the application using Cargo.

    ```bash
    cargo run
    ```

The registry will now be running and listening on `127.0.0.1:8968`.

## User and Repository Management

This registry extends the OCI specification with a user management and authentication layer.

### 1. User Registration

To push images to private repositories, you must first create a user.

*   **Endpoint**: `POST /api/v1/users`
*   **Request Body**:
    ```json
    {
        "username": "myuser",
        "password": "mypassword"
    }
    ```
*   **Response**: `201 Created` on success.

### 2. Authentication

The registry uses JWT for authenticating API requests. To obtain a token, use the Docker-compatible `/auth/token` endpoint with HTTP Basic Authentication.

*   **Endpoint**: `GET /auth/token`
*   **Authentication**: HTTP Basic Auth (use the username and password you just registered).
*   **Example using curl**:
    ```bash
    curl -u "myuser:mypassword" "http://127.0.0.1:8968/auth/token"
    ```
*   **Response**: A JSON object containing the JWT.
    ```json
    {
      "token": "ey...",
      "access_token": "ey...",
      "expires_in": 3600,
      "issued_at": "2025-08-29T..."
    }
    ```
This token should be used as a Bearer token for subsequent requests to the OCI API (e.g., `docker login`).

### 3. Repository Visibility

Repositories can be either `public` (readable by anyone) or `private` (readable only by authenticated users, writable by owner - *authorization logic may vary*).

You can change a repository's visibility using a dedicated API endpoint.

*   **Endpoint**: `PUT /api/v1/<namespace>/<repo>/visibility`
*   **Authentication**: Bearer Token (using the JWT from the `/auth/token` endpoint).
*   **Request Body**:
    ```json
    {
        "visibility": "private" //  The `visibility` field can be either `"public"` or `"private"`.
    }
    ```   
*   **Response**: `200 OK` on success.

## Command-Line Options

While using a `.env` file is recommended, the following options can be configured via command-line arguments. These will override values set in the `.env` file.

```
Usage: distribution [OPTIONS]

Options:
      --host <HOST>            Registry listening host [env: OCI_REGISTRY_URL] [default: 127.0.0.1]
  -p, --port <PORT>            Registry listening port [env: OCI_REGISTRY_PORT] [default: 8968]
  -s, --storage <STORAGE>      Storage backend type [env: OCI_REGISTRY_STORAGE] [default: FILESYSTEM]
      --root <ROOT>            Registry root path [env: OCI_REGISTRY_ROOTDIR] [default: /var/lib/registry]
      --url <URL>              Registry url [env: OCI_REGISTRY_PUBLIC_URL] [default: http://127.0.0.1:8968]
      --database-url <DB_URL>  The database URL to connect to [env: DATABASE_URL] [default: sqlite:db/registry.db]
  -h, --help                   Print help
  -V, --version                Print version
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

```bash
git clone git@github.com:opencontainers/distribution-spec.git
```

In the `conformance` directory, apply a patch and build the test binary:

```bash
cd distribution-spec/conformance/
go test -c
```

This will produce an executable at `conformance.test`.

Next, set environment variables with the registry details. **Note**: Before running the tests, you must create a user via the API as described in the User Management section.

```bash
# Registry details
export OCI_ROOT_URL="http://127.0.0.1:8968"
export OCI_NAMESPACE="myorg/myrepo"
export OCI_CROSSMOUNT_NAMESPACE="myorg/other"

# Credentials for the user you created
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

```bash
./conformance.test
```
