#!/bin/bash

# Rkforge Registry Test Script
# Test the interaction between rkforge and distribution service, including functionality correctness and user permission isolation

# Script will exit immediately if any command fails.
set -euo pipefail

# --- Configuration ---
REGISTRY_HOST="127.0.0.1:8968"
API_URL="http://127.0.0.1:8968/api/v1"
DEBUG_URL="http://127.0.0.1:8968/debug"
AUTH_URL="http://127.0.0.1:8968/auth/token"
BASE_IMAGE="hello-world"

# --- Colors and Log Functions ---
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info() {
    echo -e "${YELLOW}[INFO] $1${NC}"
}

success() {
    echo -e "${GREEN}[SUCCESS] $1${NC}"
}

fail() {
    echo -e "${RED}[FAIL] $1${NC}"
    info "Performing cleanup..."
    cleanup_test_environment
    exit 1
}

debug() {
    echo -e "${BLUE}[DEBUG] $1${NC}"
}

# --- Cleanup Function ---
cleanup_test_environment() {
    info "Cleaning up test environment..."
    
    # Logout from rkforge
    ../target/debug/rkforge logout --url "$REGISTRY_HOST" > /dev/null 2>&1 || true
    
    # Clean up any test images
    docker rmi "$BASE_IMAGE" > /dev/null 2>&1 || true
    docker rmi "${REGISTRY_HOST}/usera-${TIMESTAMP}/test-image:v1" > /dev/null 2>&1 || true
    docker rmi "${REGISTRY_HOST}/userb-${TIMESTAMP}/private-repo:v1" > /dev/null 2>&1 || true
    docker rmi "${REGISTRY_HOST}/userb-${TIMESTAMP}/public-repo:v1" > /dev/null 2>&1 || true
    
    info "Cleanup completed"
}

# --- Prerequisites Check ---
check_prerequisites() {
    info "Checking prerequisites..."
    
    # Check if distribution service is running
    if ! curl -s "$API_URL" > /dev/null; then
        fail "Distribution service is not running. Please start it first: docker-compose up -d"
    fi
    
    # Check if rkforge binary exists
    if [ ! -f "../target/debug/rkforge" ]; then
        info "Building rkforge binary..."
        cargo build --bin rkforge || fail "Failed to build rkforge"
    fi
    
    # Check for jq dependency
    if ! command -v jq &> /dev/null; then
        fail "jq is required. Please install it: sudo apt-get install jq or brew install jq"
    fi
    
    # Check for docker
    if ! command -v docker &> /dev/null; then
        fail "Docker is required"
    fi
    
    success "Prerequisites check passed"
}

# --- Test Image Preparation ---
prepare_test_image() {
    info "Preparing test image..."
    docker pull "$BASE_IMAGE" > /dev/null || fail "Failed to pull base image '$BASE_IMAGE'"
    success "Test image preparation completed"
}

# --- User Registration Functions ---
register_user() {
    local username="$1"
    local password="$2"
    
    info "Registering user: '$username'..."
    curl -s -f -X POST -H "Content-Type: application/json" \
        -d "{\"username\": \"$username\", \"password\": \"$password\"}" \
        "$DEBUG_URL/users" > /dev/null || fail "Failed to register user $username"
    success "User $username registered successfully"
}

# --- Rkforge Authentication Functions ---
rkforge_login_with_token() {
    local username="$1"
    local password="$2"
    local url="$3"
    
    info "Logging in user $username to rkforge..."
    
    # Get JWT token using curl
    local token_response
    token_response=$(curl -s -u "$username:$password" "$AUTH_URL" || fail "Failed to get JWT token")
    local jwt_token
    jwt_token=$(echo "$token_response" | jq -r .token)
    
    if [ -z "$jwt_token" ] || [ "$jwt_token" == "null" ]; then
        fail "Could not retrieve valid JWT token"
    fi
    
    # Create auth config for rkforge BEFORE any rkforge commands that might request sudo
    local config_dir="$HOME/.config/rk8s"
    mkdir -p "$config_dir"
    
    cat > "$config_dir/rkforge.toml" <<EOF
[[entries]]
pat = "$jwt_token"
url = "$url"
EOF
    
    success "User $username logged in to rkforge successfully"
}

# Fix ownership of rkforge config files that might have been created as root
fix_rkforge_config_ownership() {
    if [ -f "$HOME/.config/rk8s/rkforge.toml" ]; then
        sudo chown "$USER:$USER" "$HOME/.config/rk8s/rkforge.toml" 2>/dev/null || true
    fi
}

rkforge_logout() {
    local url="$1"
    info "Logging out from rkforge..."
    ../target/debug/rkforge logout --url "$url" > /dev/null 2>&1 || true
    
    # Fix ownership of config file if it was created as root
    fix_rkforge_config_ownership
    
    success "Logged out from rkforge successfully"
}

# --- Script Start ---
info "Starting Rkforge Advanced Distribution Server Test..."

# Setup
check_prerequisites
prepare_test_image

# Generate test users
TIMESTAMP=$(date +%s)
USER_A_NAME="usera-$TIMESTAMP"
USER_A_PASS="password"
USER_B_NAME="userb-$TIMESTAMP"
USER_B_PASS="P@ssw0rdB-$TIMESTAMP"

# ==============================================================================
# SETUP FOR MULTI-USER TESTS
# ==============================================================================
info "\n--- Setting up for Multi-User Tests ---"

# Register users
register_user "$USER_A_NAME" "$USER_A_PASS"
register_user "$USER_B_NAME" "$USER_B_PASS"

# ==============================================================================
# TEST CASE 1: ANONYMOUS USER TEST
# ==============================================================================
info "\n--- Running Test Case 1: Anonymous User Permissions ---"

# Ensure we are logged out and clean up any existing config
rkforge_logout "$REGISTRY_HOST"
rm -f "$HOME/.config/rk8s/rkforge.toml" 2>/dev/null || true

ANONYMOUS_IMAGE_TAG="anonymous/test:v1"
docker tag "$BASE_IMAGE" "$REGISTRY_HOST/$ANONYMOUS_IMAGE_TAG"

info "Attempting to push '$ANONYMOUS_IMAGE_TAG' as anonymous user (this SHOULD fail)..."
if sudo ../target/debug/rkforge push --url "$REGISTRY_HOST" --path "output/image1" "$ANONYMOUS_IMAGE_TAG" > /dev/null 2>&1; then
    fail "SECURITY RISK: Anonymous user was able to push an image!"
else
    success "Push correctly failed for anonymous user as expected"
fi

# Fix ownership of any config files created by rkforge
fix_rkforge_config_ownership

# ==============================================================================
# TEST CASE 2: CROSS-NAMESPACE PUSH PERMISSIONS
# ==============================================================================
info "\n--- Running Test Case 2: Cross-Namespace Push Permissions ---"

# Login as User A
rkforge_login_with_token "$USER_A_NAME" "$USER_A_PASS" "$REGISTRY_HOST"

# User A pushes to their own namespace (should succeed)
USER_A_IMAGE_TAG="$USER_A_NAME/test-image:v1"
docker tag "$BASE_IMAGE" "$REGISTRY_HOST/$USER_A_IMAGE_TAG"

info "User A attempting to push to their own namespace '$USER_A_IMAGE_TAG'..."
if sudo ../target/debug/rkforge push --url "$REGISTRY_HOST" --path "output/image1" "$USER_A_IMAGE_TAG" > /dev/null 2>&1; then
    success "User A successfully pushed to their own namespace"
else
    fail "User A failed to push to their own namespace"
fi

# User A tries to push to User B's namespace (should fail)
USER_B_IMAGE_TAG_ATTEMPT="$USER_B_NAME/illegal-push:v1"
docker tag "$BASE_IMAGE" "$REGISTRY_HOST/$USER_B_IMAGE_TAG_ATTEMPT"

info "User A attempting to push to User B's namespace '$USER_B_IMAGE_TAG_ATTEMPT' (this SHOULD fail)..."
if sudo ../target/debug/rkforge push --url "$REGISTRY_HOST" --path "output/image1" "$USER_B_IMAGE_TAG_ATTEMPT" > /dev/null 2>&1; then
    fail "SECURITY RISK: User A was able to push to User B's namespace!"
else
    success "Push correctly failed as User A cannot push to User B's namespace"
fi

# Logout User A
rkforge_logout "$REGISTRY_HOST"

# ==============================================================================
# TEST CASE 3: PRIVATE/PUBLIC PULL PERMISSIONS
# ==============================================================================
info "\n--- Running Test Case 3: Private/Public Pull Permissions ---"

# Login as User B to setup repositories
rkforge_login_with_token "$USER_B_NAME" "$USER_B_PASS" "$REGISTRY_HOST"

# Push to create a private repository
PRIVATE_REPO_TAG="$USER_B_NAME/private-repo:v1"
docker tag "$BASE_IMAGE" "$REGISTRY_HOST/$PRIVATE_REPO_TAG"

info "User B pushing to create a private repository..."
if sudo ../target/debug/rkforge push --url "$REGISTRY_HOST" --path "output/image1" "$PRIVATE_REPO_TAG" > /dev/null 2>&1; then
    success "User B created a private repository"
else
    fail "User B failed to create private repository"
fi

# Push to create a public repository
PUBLIC_REPO_TAG="$USER_B_NAME/public-repo:v1"
docker tag "$BASE_IMAGE" "$REGISTRY_HOST/$PUBLIC_REPO_TAG"

info "User B pushing to create a soon-to-be-public repository..."
if sudo ../target/debug/rkforge push --url "$REGISTRY_HOST" --path "output/image2" "$PUBLIC_REPO_TAG" > /dev/null 2>&1; then
    success "User B created a soon-to-be-public repository"
else
    fail "User B failed to create public repository"
fi

# Get JWT for User B to make the API call
info "Getting JWT for User B to set repository visibility..."
USER_B_TOKEN=$(curl -s -u "$USER_B_NAME:$USER_B_PASS" "$AUTH_URL" | jq -r .token)
if [ -z "$USER_B_TOKEN" ] || [ "$USER_B_TOKEN" == "null" ]; then
    fail "Could not retrieve JWT for User B"
fi

# Set repository to public using API
info "User B setting repository '$USER_B_NAME/public-repo' to public..."
curl -s -f -X PUT \
  -H "Authorization: Bearer $USER_B_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"visibility": "public"}' \
  "$API_URL/$USER_B_NAME/public-repo/visibility" || fail "API call to set repo to public failed"
success "User B created and set a public repository"

# Logout User B
rkforge_logout "$REGISTRY_HOST"

# Test: User A tries to pull from User B's repos
info "Logging in as User A to test pull permissions..."
rkforge_login_with_token "$USER_A_NAME" "$USER_A_PASS" "$REGISTRY_HOST"

# Clean local cache for real test
docker rmi "$REGISTRY_HOST/$PRIVATE_REPO_TAG" > /dev/null 2>&1 || true
docker rmi "$REGISTRY_HOST/$PUBLIC_REPO_TAG" > /dev/null 2>&1 || true

# User A tries to pull User B's private repo (should fail)
info "User A attempting to pull User B's private repo '$PRIVATE_REPO_TAG' (this SHOULD fail)..."
if sudo ../target/debug/rkforge pull --url "$REGISTRY_HOST" "$PRIVATE_REPO_TAG" > /dev/null 2>&1; then
    fail "SECURITY RISK: User A was able to pull User B's private repo!"
else
    success "Pull correctly failed as User A cannot access User B's private repo"
fi

# User A tries to pull User B's public repo (should succeed)
info "User A attempting to pull User B's public repo '$PUBLIC_REPO_TAG'..."
if sudo ../target/debug/rkforge pull --url "$REGISTRY_HOST" "$PUBLIC_REPO_TAG" > /dev/null 2>&1; then
    success "User A successfully pulled User B's public repo"
else
    fail "User A failed to pull User B's public repo"
fi

# Logout User A
rkforge_logout "$REGISTRY_HOST"

# ==============================================================================
# TEST CASE 4: Rkforge SPECIFIC FUNCTIONALITY TESTS
# ==============================================================================
info "\n--- Running Test Case 4: Rkforge Specific Functionality Tests ---"

# Test rkforge repo commands
rkforge_login_with_token "$USER_A_NAME" "$USER_A_PASS" "$REGISTRY_HOST"

info "Testing rkforge repo list command..."
repo_list_output=$(../target/debug/rkforge repo --url "$REGISTRY_HOST" list 2>&1)
if echo "$repo_list_output" | grep -q "repository.*visibility"; then
    success "rkforge repo list command executed successfully and shows repository table"
    
    # Check if it shows the expected repositories
    if echo "$repo_list_output" | grep -q "$USER_A_NAME/test-image"; then
        success "rkforge repo list shows User A's test-image repository"
    else
        info "rkforge repo list output: $repo_list_output"
        fail "rkforge repo list does not show User A's test-image repository"
    fi
    
    if echo "$repo_list_output" | grep -q "$USER_B_NAME/public-repo"; then
        success "rkforge repo list shows User B's public repository"
    else
        info "rkforge repo list output: $repo_list_output"
        fail "rkforge repo list does not show User B's public repository"
    fi
else
    fail "rkforge repo list command failed or did not show expected output. Output: $repo_list_output"
fi

info "Testing rkforge repo vis command..."
if ../target/debug/rkforge repo --url "$REGISTRY_HOST" vis "$USER_A_NAME/test-image" "public" > /dev/null 2>&1; then
    success "rkforge repo vis command executed successfully"
    
    # Verify the visibility change by listing repositories again
    info "Verifying visibility change..."
    repo_list_after_vis=$(../target/debug/rkforge repo --url "$REGISTRY_HOST" list 2>&1)
    if echo "$repo_list_after_vis" | grep -q "$USER_A_NAME/test-image.*public"; then
        success "Repository visibility successfully changed to public"
    else
        info "Repository list after visibility change: $repo_list_after_vis"
        fail "Repository visibility change was not reflected in the list"
    fi
else
    fail "rkforge repo vis command failed"
fi

# Test pull of previously pushed image
info "Testing pull of previously pushed image..."
docker rmi "$REGISTRY_HOST/$USER_A_IMAGE_TAG" > /dev/null 2>&1 || true

if sudo ../target/debug/rkforge pull --url "$REGISTRY_HOST" "$USER_A_IMAGE_TAG" > /dev/null 2>&1; then
    success "Successfully pulled previously pushed image"
else
    fail "Failed to pull previously pushed image"
fi

rkforge_logout "$REGISTRY_HOST"

# ==============================================================================
# TEST COMPLETE
# ==============================================================================
cleanup_test_environment
echo
success "All Rkforge advanced tests passed successfully!"
info "Test Summary:"
info "✅ Anonymous user permission isolation correct"
info "✅ Cross-namespace push permission isolation correct"
info "✅ Private/public repository pull permissions correct"
info "✅ Rkforge specific functionality working properly"
