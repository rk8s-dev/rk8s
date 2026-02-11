# trust-dns-{client,server} not available
# using vendored deps

%global with_debug 1

%if 0%{?with_debug}
%global _find_debuginfo_dwz_opts %{nil}
%global _dwz_low_mem_die_limit 0
%else
%global debug_package %{nil}
%endif

Name: aardvark-dns
%if %{defined copr_username}
Epoch: 102
%else
Epoch: 2
%endif
# DO NOT TOUCH the Version string!
# The TRUE source of this specfile is:
# https://github.com/containers/podman/blob/main/rpm/podman.spec
# If that's what you're reading, Version must be 0, and will be updated by Packit for
# copr and koji builds.
# If you're reading this on dist-git, the version is automatically filled in by Packit.
Version: 0
# The `AND` needs to be uppercase in the License for SPDX compatibility
License: Apache-2.0 AND MIT AND Zlib
Release: %autorelease
%if %{defined golang_arches_future}
ExclusiveArch: %{golang_arches_future}
%else
ExclusiveArch: aarch64 ppc64le s390x x86_64
%endif
Summary: Authoritative DNS server for A/AAAA container records
URL: https://github.com/containers/%{name}
# Tarballs fetched from upstream's release page
Source0: %{url}/archive/v%{version}.tar.gz
Source1: %{url}/releases/download/v%{version}/%{name}-v%{version}-vendor.tar.gz
BuildRequires: cargo
BuildRequires: git-core
BuildRequires: make
%if %{defined rhel}
# rust-toolset requires the `local` repo enabled on non-koji ELN build environments
BuildRequires: rust-toolset
%else
BuildRequires: rust-packaging
BuildRequires: rust-srpm-macros
%endif

%description
%{summary}

Forwards other request to configured resolvers.
Read more about configuration in `src/backend/mod.rs`.

%package tests
Summary: Tests for %{name}

Requires: %{name} = %{epoch}:%{version}-%{release}
Requires: bats
Requires: bind-utils
Requires: jq
Requires: netavark
Requires: socat
Requires: dnsmasq

%description tests
%{summary}

This package contains system tests for %{name} and is only intended to be used
for gating tests.

%prep
%autosetup -Sgit %{name}-%{version}
# Following steps are only required on environments like koji which have no
# network access and thus depend on the vendored tarball. Copr pulls
# dependencies directly from the network.
%if !%{defined copr_username}
tar fx %{SOURCE1}
%if 0%{?fedora} || 0%{?rhel} >= 10
%cargo_prep -v vendor
%else
%cargo_prep -V 1
%endif
%endif

%build
%{__make} CARGO="%{__cargo}" build
%if (0%{?fedora} || 0%{?rhel} >= 10) && !%{defined copr_username}
%cargo_license_summary
%{cargo_license} > LICENSE.dependencies
%cargo_vendor_manifest
%endif

%install
%{__make} DESTDIR=%{buildroot} PREFIX=%{_prefix} install

%{__install} -d -p %{buildroot}%{_datadir}/%{name}/test
%{__cp} -rp test/* %{buildroot}%{_datadir}/%{name}/test/
%{__rm} -rf %{buildroot}%{_datadir}/%{name}/test/tmt/

# Add empty check section to silence rpmlint warning.
# No tests meant to be run here.
%check

%files
%license LICENSE
%if (0%{?fedora} || 0%{?rhel} >= 10) && !%{defined copr_username}
%license LICENSE.dependencies
%license cargo-vendor.txt
%endif
%dir %{_libexecdir}/podman
%{_libexecdir}/podman/%{name}

%files tests
%{_datadir}/%{name}/test

%changelog
%autochangelog
