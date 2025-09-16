#!/bin/bash

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
    docker logout "$REGISTRY_HOST" > /dev/null 2>&1 || true
    exit 1
}

# --- Script Start ---
info "Starting Advanced Distribution Server Test..."

# Check for jq dependency
if ! command -v jq &> /dev/null; then
    fail "jq could not be found. Please install it to run this script (e.g., 'sudo apt-get install jq' or 'brew install jq')."
fi

# 1. Prepare base image
info "Ensuring local copy of '$BASE_IMAGE' image exists..."
docker pull "$BASE_IMAGE" > /dev/null || fail "Failed to pull base image '$BASE_IMAGE'."


# ==============================================================================
# TEST CASE 1: ANONYMOUS USER (Unchanged)
# ==============================================================================
info "\n--- Running Test Case 1: Anonymous User Permissions ---"
info "Ensuring we are logged out from $REGISTRY_HOST..."
docker logout "$REGISTRY_HOST" > /dev/null 2>&1 || true
ANONYMOUS_IMAGE_TAG="$REGISTRY_HOST/anonymous/test:v1"
docker tag "$BASE_IMAGE" "$ANONYMOUS_IMAGE_TAG"
info "Attempting to push '$ANONYMOUS_IMAGE_TAG' as an anonymous user (this SHOULD fail)..."
if ! docker push "$ANONYMOUS_IMAGE_TAG" > /dev/null 2>&1; then
    success "Push correctly failed for anonymous user as expected."
else
    fail "SECURITY RISK: Anonymous user was able to push an image!"
fi


# ==============================================================================
# SETUP FOR MULTI-USER TESTS
# ==============================================================================
info "\n--- Setting up for Multi-User Tests ---"

# Generate two distinct random users
TIMESTAMP=$(date +%s)
USER_A_NAME="usera-$TIMESTAMP"
USER_A_PASS="password"
USER_B_NAME="userb-$TIMESTAMP"
USER_B_PASS="P@ssw0rdB-$TIMESTAMP"

# Register User A
info "Registering User A: '$USER_A_NAME'..."
curl -s -f -X POST -H "Content-Type: application/json" -d "{\"username\": \"$USER_A_NAME\", \"password\": \"$USER_A_PASS\"}" "$DEBUG_URL/users" > /dev/null || fail "Failed to register User A."
success "User A registered."

# Register User B
info "Registering User B: '$USER_B_NAME'..."
curl -s -f -X POST -H "Content-Type: application/json" -d "{\"username\": \"$USER_B_NAME\", \"password\": \"$USER_B_PASS\"}" "$DEBUG_URL/users" > /dev/null || fail "Failed to register User B."
success "User B registered."


# ==============================================================================
# TEST CASE 2: CROSS-NAMESPACE PUSH PERMISSIONS
# ==============================================================================
info "\n--- Running Test Case 2: Cross-Namespace Push Permissions ---"
# Login as User A
info "Logging in as User A..."
echo "$USER_A_PASS" | docker login "$REGISTRY_HOST" -u "$USER_A_NAME" --password-stdin || fail "Login failed for User A."

# User A pushes to their own namespace (should succeed)
USER_A_IMAGE_TAG="$REGISTRY_HOST/$USER_A_NAME/test-image:v1"
docker tag "$BASE_IMAGE" "$USER_A_IMAGE_TAG"
info "User A attempting to push to their own namespace '$USER_A_IMAGE_TAG'..."
docker push "$USER_A_IMAGE_TAG" || fail "User A failed to push to their own namespace."
success "User A successfully pushed to their own namespace."

# User A tries to push to User B's namespace (should fail)
USER_B_IMAGE_TAG_ATTEMPT="$REGISTRY_HOST/$USER_B_NAME/illegal-push:v1"
docker tag "$BASE_IMAGE" "$USER_B_IMAGE_TAG_ATTEMPT"
info "User A attempting to push to User B's namespace '$USER_B_IMAGE_TAG_ATTEMPT' (this SHOULD fail)..."
if ! docker push "$USER_B_IMAGE_TAG_ATTEMPT" > /dev/null 2>&1; then
    success "Push correctly failed as User A cannot push to User B's namespace."
else
    fail "SECURITY RISK: User A was able to push to User B's namespace!"
fi

# Logout User A
docker logout "$REGISTRY_HOST" > /dev/null 2>&1


# ==============================================================================
# TEST CASE 3: PRIVATE/PUBLIC PULL PERMISSIONS
# ==============================================================================
info "\n--- Running Test Case 3: Private/Public Pull Permissions ---"
# --- Setup: User B creates a private and a public repository ---
info "Setting up repositories with User B..."
echo "$USER_B_PASS" | docker login "$REGISTRY_HOST" -u "$USER_B_NAME" --password-stdin || fail "Login failed for User B."

# Push to create a repo that will remain private
PRIVATE_REPO_TAG="$REGISTRY_HOST/$USER_B_NAME/private-repo:v1"
docker tag "$BASE_IMAGE" "$PRIVATE_REPO_TAG"
info "User B pushing to create a private repository..."
docker push "$PRIVATE_REPO_TAG" || fail "User B failed to push their private repo image."
success "User B created a private repository."

# Push to create a repo that will be made public
PUBLIC_REPO_TAG="$REGISTRY_HOST/$USER_B_NAME/public-repo:v1"
docker tag "$BASE_IMAGE" "$PUBLIC_REPO_TAG"
info "User B pushing to create a soon-to-be-public repository..."
docker push "$PUBLIC_REPO_TAG" || fail "User B failed to push their public repo image."

# Get JWT for User B to make the API call
info "Getting JWT for User B to set repository visibility..."
USER_B_TOKEN=$(curl -s -u "$USER_B_NAME:$USER_B_PASS" "$AUTH_URL" | jq -r .token)
if [ -z "$USER_B_TOKEN" ] || [ "$USER_B_TOKEN" == "null" ]; then
    fail "Could not retrieve JWT for User B."
fi

# Use API to set the repository to public
info "User B setting repository '$USER_B_NAME/public-repo' to public..."
curl -s -f -X PUT \
  -H "Authorization: Bearer $USER_B_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"visibility": "public"}' \
  "$API_URL/$USER_B_NAME/public-repo/visibility" || fail "API call to set repo to public failed."
success "User B created and set a public repository."

# Logout User B
docker logout "$REGISTRY_HOST" > /dev/null 2>&1

# --- Test: User A tries to pull from User B's repos ---
info "Logging in as User A to test pull permissions..."
echo "$USER_A_PASS" | docker login "$REGISTRY_HOST" -u "$USER_A_NAME" --password-stdin || fail "Login failed for User A."

# Clean local cache for a real test
docker rmi "$PRIVATE_REPO_TAG" > /dev/null 2>&1 || true
docker rmi "$PUBLIC_REPO_TAG" > /dev/null 2>&1 || true

# User A tries to pull User B's private repo (should fail)
info "User A attempting to pull User B's private repo '$PRIVATE_REPO_TAG' (this SHOULD fail)..."
if ! docker pull "$PRIVATE_REPO_TAG" > /dev/null 2>&1; then
    success "Pull correctly failed as User A cannot access User B's private repo."
else
    fail "SECURITY RISK: User A was able to pull User B's private repo!"
fi

# User A tries to pull User B's public repo (should succeed)
info "User A attempting to pull User B's public repo '$PUBLIC_REPO_TAG'..."
docker pull "$PUBLIC_REPO_TAG" || fail "User A failed to pull User B's public repo."
success "User A successfully pulled User B's public repo."

# Logout User A
docker logout "$REGISTRY_HOST" > /dev/null 2>&1

# ==============================================================================
# TEST COMPLETE
# ==============================================================================
echo
success "All advanced tests passed successfully!"
