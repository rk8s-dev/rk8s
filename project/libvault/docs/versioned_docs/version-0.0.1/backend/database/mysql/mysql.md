# MySQL Optimization with Diesel CLI in Rust

This document outlines the process of setting up and using `diesel_cli` with MySQL in Rust, and discusses potential areas for optimization.

## Using Diesel CLI with MySQL in Rust

`diesel_cli` is an ORM (Object-Relational Mapping) framework that enables the generation of structs and DSL (Domain Specific Language) from SQL files. The following steps guide you through setting up and using `diesel_cli` with MySQL in your Rust project.

### Step 1: Environment Setup

Firstly, define the `MYSQLCLIENT_LIB_DIR` environment variable. The process varies depending on your platform:

- **Linux**:
    ```shell
    export MYSQLCLIENT_LIB_DIR="your path to mysqlclient.lib"
    ```

- **Windows**:
    ```shell
    setx MYSQLCLIENT_LIB_DIR "your path to mysqlclient.lib"
    ```

- **GitHub Actions on Windows**:
    ```shell
    - run: echo "MYSQLCLIENT_LIB_DIR=C:\Program Files\MySQL\MySQL Connector C 6.1\lib\vs14" | Out-File -FilePath $env:GITHUB_ENV -Append
    ```

> Note: If you do not only set the `mysqlclient.lib` file, it may result in duplicate library errors during compilation.

### Step 2: Install Diesel CLI
Install `diesel_cli` using the `cargo` command:

```shell
cargo install diesel_cli --no-default-features --features mysql
```

### Step 3: Import Diesel into the Project

Add the following dependencies to your `Cargo.toml`:

```toml
[dependencies]
# other dependencies
diesel = { version = "2.1.4", features = ["mysql", "r2d2"] }
r2d2 = "0.8.9"
r2d2-diesel = "1.0.0"
```

### Step 4: Generate Structs with Diesel CLI

Use `diesel_cli` to set up your database and generate migrations:

```shell
cd /path/to/project/root
diesel setup --database-url="mysql://[username:[password]]@[host:[port]]/[database]"
diesel migration generate your_sql_summary --database-url="mysql://[username:[password]]@[host:[port]]/[database]"
diesel migration run "mysql://[username:[password]]@[host:[port]]/[database]"
```

Run the unit test with `mysqlbackend`.

## Potential Optimization Areas

- Establishing a TLS connection to MySQL
- Connecting to PostgreSQL
