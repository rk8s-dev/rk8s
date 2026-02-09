# Aardvark-dns: Authoritative DNS server for A/AAAA container records

This document is currently written with Fedora as a reference. As Aardvark-dns
gets shipped in other distros, this should become a distro-agnostic
document.

## Fedora Users
Aardvark-dns is available as an official Fedora package on Fedora 35 and newer versions
and is only meant to be used with Podman v4 and newer releases. On Fedora 36
and newer, fresh installations of the podman package will automatically install
Aardvark-dns along with Netavark. If Aardvark-dns isn't present on your system,
install it using:

```console
$ sudo dnf install aardvark-dns
```

**NOTE:** Fedora 35 users will not be able to install Podman v4 using the default yum
repositories. Please consult the Podman packaging docs for instructions on how
to fetch Podman v4.0 on Fedora 35.

If you would like to test the latest unreleased upstream code, try the
podman-next COPR:

```console
$ sudo dnf copr enable rhcontainerbot/podman-next

$ sudo dnf install aardvark-dns
```

**CAUTION:** The podman-next COPR provides the latest unreleased sources of Podman,
Aardvark-dns and Aardvark-dns as rpms which would override the versions provided by
the official packages.

## Distro Packagers

The Fedora packaging sources for Aardvark-dns are available at the [Aardvark-dns
dist-git](https://src.fedoraproject.org/rpms/aardvark-dns).

The Fedora package builds Aardvark-dns using a compressed tarball of the vendored
libraries that is attached to each upstream release.
You can download them with the following:

`https://github.com/containers/netavark/releases/download/v{version}/aardvark-dns-v{version}.tar.gz`

And then create a cargo config file to point it to the vendor dir:
```
tar xvf %{SOURCE}
mkdir -p .cargo
cat >.cargo/config << EOF
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
EOF
```

The `aardvark-dns` binary is installed to `/usr/libexec/podman/aardvark-dns`.

## Dependency of netavark package
The netavark package has a `Recommends` on the `aardvark-dns` package. The
aardvark-dns package will be installed by default with netavark, but Netavark
and Podman will be functional without it.

## Listing bundled dependencies
If you need to list the bundled dependencies in your packaging sources, you can
run the `cargo tree` command in the upstream source.
For example, Fedora's packaging source uses:

```
$ cargo tree --prefix none | awk '{print "Provides: bundled(crate("$1")) = "$2}' | sort | uniq
```
