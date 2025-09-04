rust_binary(
    name = "rk8s",
    srcs = glob(
        ["project/tools/scripts/main.rs"],
    ),
    crate_root = "project/tools/scripts/main.rs",
    deps = [
        "root//project/distribution:distribution",
        "root//project/libbridge:libbridge",
        "root//project/libcgroups:libcgroups",
        "root//project/libcni:libcni",
        "root//project/libcontainer:libcontainer",
        "root//project/libfuse-fs:libfuse-fs",
        "root//project/libipam:libipam",
        "root//project/libnetwork:libnetwork",
        "root//project/libscheduler:libscheduler",
        "root//project/rkb:rkb",
        "root//project/rks:rks",
        "root//project/rkl:rkl_bin",
        "root//project/rfuse3:rfuse3",
        "root//project/common:common",
    ],
)