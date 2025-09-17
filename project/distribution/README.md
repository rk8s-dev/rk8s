# Distribution

A lite implementation of the OCI Distribution Spec in Rust, with added support for user management and authentication.

## Configuration

The registry is configured via environment variables, which can be loaded from a `.env` file or command-line arguments. The application intelligently adapts its database connection based on the variables provided.

### For Local Development (using `cargo run`)

For local development, the easiest method is to provide a complete `DATABASE_URL` connection string. The application will detect and use this variable directly.

Create a `.env` file in the project's root directory:

```dotenv
# Application Host and Port
OCI_REGISTRY_URL=127.0.0.1
OCI_REGISTRY_PORT=8968

# Public URL used in responses
OCI_REGISTRY_PUBLIC_URL=http://127.0.0.1:8968

# Storage Configuration
OCI_REGISTRY_STORAGE=FILESYSTEM
OCI_REGISTRY_ROOTDIR=/var/lib/registry

# --- Database Configuration (Direct Method) ---
# Provide the full URL for local development
DATABASE_URL="postgres://postgres:password@localhost:5432/postgres"

# A secret key for signing JWT tokens.
# Generate a secure random string for production use.
JWT_SECRET="secret"

# JWT token lifetime in seconds
JWT_LIFETIME_SECONDS=3600

# GitHub OAuth Configuration (optional)
# Required for GitHub OAuth authentication
# GITHUB_CLIENT_ID="your_github_client_id"
# GITHUB_CLIENT_SECRET="your_github_client_secret"

# Log level
RUST_LOG="info"
```

### For Docker Compose (Recommended)

For the Docker Compose environment, you should provide the database connection components separately. The application will detect that `DATABASE_URL` is not set and will construct the correct connection string for the container network itself.

Create a `.env` file with the following content:

```dotenv
# ===============================================
# Docker Compose Orchestration Config
# ===============================================
APP_PORT=8968
DB_EXTERNAL_PORT=5433
POSTGRES_VERSION=15

# ===============================================
# Application Runtime Config
# ===============================================
# Bind to 0.0.0.0 to accept connections from outside the container
OCI_REGISTRY_URL=0.0.0.0
OCI_REGISTRY_PORT=8968

# Public URL accessible by clients
OCI_REGISTRY_PUBLIC_URL=http://127.0.0.1:8968

# Storage path inside the container
OCI_REGISTRY_STORAGE=FILESYSTEM
OCI_REGISTRY_ROOTDIR=/var/lib/oci-registry

# --- Database Configuration (Component Method) ---
# DO NOT set DATABASE_URL here. Provide components instead.
POSTGRES_HOST=db
POSTGRES_PORT=5432
POSTGRES_USER=postgres
POSTGRES_PASSWORD=password
POSTGRES_DB=postgres

# --- Security Configuration ---
JWT_SECRET="secret"
JWT_LIFETIME_SECONDS=3600

# GitHub OAuth Configuration (optional)
# Required for GitHub OAuth authentication
# GITHUB_CLIENT_ID="your_github_client_id"
# GITHUB_CLIENT_SECRET="your_github_client_secret"

# Log level
RUST_LOG="info"
```

**Security Note**: For production environments, `JWT_SECRET`, `POSTGRES_PASSWORD`, `GITHUB_CLIENT_ID`, and `GITHUB_CLIENT_SECRET` must be protected and set to secure values. Generate secure random strings for JWT_SECRET and obtain GitHub OAuth credentials from your GitHub application settings.

## Quick Start

### With Cargo (Local Development)

1.  **Prerequisites**: Ensure you have a PostgreSQL server running and accessible.
2.  **Configure**: Create a `.env` file for local development as described above (using `DATABASE_URL`).
3.  **Start**: Run the application using Cargo.
    ```bash
    cargo run
    ```
The registry will now be running and listening on `127.0.0.1:8968`.

### With Docker Compose (Recommended)

This is the easiest way to get started, as it manages both the application and its database.

1.  **Prerequisites**: Docker and Docker Compose must be installed.
2.  **Configure**: Create a `.env` file for Docker Compose as described above (using separate `POSTGRES_*` variables).
3.  **Start**: Use Docker Compose to build and start the services.
    ```bash
    docker-compose up --build -d
    ```
    *   `--build`: Forces a rebuild of the application image if you've made code changes.
    *   `-d`: Runs the containers in detached mode.

4.  **Check Status**: You can check if the services are running correctly.
    ```bash
    docker-compose ps
    ```

5.  **View Logs**: To see the application logs in real-time:
    ```bash
    docker-compose logs -f distribution
    ```
The registry will be running and accessible on `http://127.0.0.1:8968`.

6.  **Stopping**: To stop and remove the containers:
    ```bash
    docker-compose down
    ```

## User and Repository Management

This registry extends the OCI specification with a user management and authentication layer.

### 1. User Registration

#### Debug Mode Registration (Development Only)

For development purposes, you can create users directly via the debug API (only available when compiled with debug assertions):

*   **Endpoint**: `POST /debug/users`
*   **Request Body**:
    ```json
    {
        "username": "myuser",
        "password": "mypassword"
    }
    ```
*   **Response**: `201 Created` on success.

#### OAuth Registration (GitHub)

The registry supports OAuth authentication through GitHub. To use this feature, you must configure GitHub OAuth credentials in your environment variables.

*   **Endpoint**: `GET /api/v1/auth/github/callback?code=<oauth_code>`
*   **Purpose**: Handle GitHub OAuth callback and create/authenticate users
*   **Prerequisites**: Set `GITHUB_CLIENT_ID` and `GITHUB_CLIENT_SECRET` environment variables
*   **Response**: Returns a Personal Access Token (PAT)
    ```json
    {
      "pat": "ey..."
    }
    ```

### 2. Authentication

The registry uses JWT for authenticating API requests. To obtain a token, use the Docker-compatible `/auth/token` endpoint with HTTP Basic Authentication.

*   **Endpoint**: `GET /auth/token`
*   **Authentication**: HTTP Basic Auth (use the username and password you registered).
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
      "issued_at": "2025-09-17T..."
    }
    ```
This token should be used as a Bearer token for subsequent requests to the OCI API (e.g., `docker login`).

### 3. Repository Management

#### List Visible Repositories

List all repositories visible to the authenticated user:

*   **Endpoint**: `GET /api/v1/repo`
*   **Authentication**: Bearer Token (JWT from `/auth/token` endpoint)
*   **Response**: 
    ```json
    {
      "data": [
        {
          "namespace": "myuser",
          "name": "myrepo",
          "is_public": true
        }
      ]
    }
    ```

#### Change Repository Visibility

Repositories can be either `public` (readable by anyone) or `private` (readable only by authenticated users, writable by owner).

*   **Endpoint**: `PUT /api/v1/<namespace>/<repo>/visibility`
*   **Authentication**: Bearer Token (using the JWT from the `/auth/token` endpoint).
*   **Request Body**:
    ```json
    {
        "visibility": "private"
    }
    ```   
*   **Response**: `200 OK` on success.
*   **Note**: The `visibility` field can be either `"public"` or `"private"`.

## Command-Line Options

While using a `.env` file is recommended, configuration can be overridden via command-line arguments. Based on the new configuration logic, the database can be configured with component flags.

```
Usage: distribution [OPTIONS]

Options:
      --host <HOST>        Registry listening host [env: OCI_REGISTRY_URL] [default: 127.0.0.1]
  -p, --port <PORT>        Registry listening port [env: OCI_REGISTRY_PORT] [default: 8968]
  -s, --storage <STORAGE>  Storage backend type [env: OCI_REGISTRY_STORAGE] [default: FILESYSTEM]
      --root <ROOT>        Registry root path [env: OCI_REGISTRY_ROOTDIR] [default: /var/lib/registry]
      --url <URL>          Registry url [env: OCI_REGISTRY_PUBLIC_URL] [default: http://127.0.0.1:8968]
      --db-host <DB_HOST>  Database host [env: POSTGRES_HOST] [default: localhost]
      --db-port <DB_PORT>  Database port [env: POSTGRES_PORT] [default: 5432]
      --db-user <DB_USER>  Database user [env: POSTGRES_USER] [default: postgres]
      --db-name <DB_NAME>  Database name [env: POSTGRES_DB] [default: postgres]
  -h, --help               Print help
  -V, --version            Print version
```
**Note**: The database password is intentionally not exposed as a command-line argument for security reasons. It must be provided via the `POSTGRES_PASSWORD` environment variable if `DATABASE_URL` is not set.

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