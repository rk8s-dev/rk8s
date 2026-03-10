fn main() {
    // No longer need to use tonic_build to compile proto; the generated code has been checked into the repository.
    // If there are no other build tasks in the future, this file can be completely deleted.
    println!("cargo:warning=Proto generation is skipped using checked-in sources");
}
