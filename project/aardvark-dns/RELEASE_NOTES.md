# Release Notes

## v1.17.0

* Aardvark-dns now updates the upstream nameservers from /etc/resolv.conf when the file content changes using inotify. This means a container restart is no longer required to re-read resolv.conf.
* Dependency updates.

## v1.16.0

* Allow more than one DNS message per tcp socket. ([#605](https://github.com/containers/aardvark-dns/issues/605))
* Dependency updates.

## v1.15.0

* Dependency updates.

## v1.14.0

* Dependency updates.

## v1.13.1

* Fix parsing of ipv6 link local addresses in resolv.conf ([#535](https://github.com/containers/aardvark-dns/issues/535))

## v1.13.0

* Set TTL to 0 for container names
* Allow forwarding of names with no ndots
* DNS: limit to 3 resolvers and use better timeout for them
* Ignore unknown resolv.conf options

## v1.12.2

* This releases fixes a security issue (CVE-2024-8418) where tcp connections where not handled correctly which allowed a container to block dns queries for other clients on the same network #500. Versions before v1.12.0 are unaffected as they do not have tcp support.

## v1.12.1

* Fixed problem with categories in Cargo.toml that prevented us from publishing v1.12.0

## v1.12.0

* Dependency updates
* Improve all around error handling and logging
* Added TCP/IP support
* Update upsteam resolvers on each refresh

## v1.11.0
* Do not allow "internal" networks to access DNS
* On SIGHUP, stop AV threads no longer needed and reload in memory those that are
* updated dependencies

## v1.10.0
* removed unused kill switch
* updated dependencies

## v1.9.0
* update trust-dns to hickory
* never report an error when the syslog init fails
* dependency updates

## v1.8.0
* dependency updates

## v1.7.0
* dependency updates

## v1.6.0
* dependency updates
* lower the TTL to 60s for container names

## v1.5.0
* dependency updates
* code of conduct added

## v1.4.0
* Add support for network scoped dns servers; declare DNS at a network level

## v1.3.0
* allow one or more dns servers in the aardvark config

## v1.2.0
* coredns: do not combine results of A and AAAA records
* run,serve: create aardvark pid in child before we notify parent process
* coredns: response message set recursion available if RD is true
* document configuration format

## v1.1.0
* Changed Aardvark to fork on startup to daemonize, as opposed to have this done by callers. This avoids race conditions around startup.
* Name resolution is now case-insensitive.

## v1.0.3
* Updated dependancy libraries
* Reduction in CPU use
* Fixed bug with duplicate network names

## v1.0.2
* Updated dependency libraries
* Removed vergen dependency

## v1.0.1
- Remove vendor directory from upstream github repository
- Vendored libraries updates

## v1.0.0
- First release of aardvark-dns.

## v1.0.0-RC2
- Slew of bug fixes related to reverse lookups, NXDOMAIN returns, and so on. Getting very close to first official release.

## v1.0.0-RC1
- This is the first release candidate of Aardvark's initial release! All major functionality is implemented and working.
