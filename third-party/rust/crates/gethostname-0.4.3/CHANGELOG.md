# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.3] – 2023-05-13

### Changed
- Removed the `windows` dependency in favor of using embedded bindings, see [GH-11].

[GH-11]: https://github.com/swsnr/gethostname.rs/pull/11

## [0.4.2] – 2023-04-13

### Changed
- Update dependencies.

## [0.4.1] – 2022-12-01

### Changed

- Update repository URL to <https://github.com/swsnr/gethostname.rs>.

## [0.4.0] – 2022-10-28

### Changed
- Replace `winapi` with windows-rs, see [GH-7].
- Bump MSRV to 1.64 as required by windows-rs, see [GH-7].

[GH-7]: https://github.com/swsnr/gethostname.rs/pull/7

## [0.3.0] – 2022-10-09

### Changed
- Bump MSRV to 1.56.

## [0.2.3] – 2022-03-12

### Changed
- Limit `gethostname()` to `cfg(unix)` and `cfg(windows)` to provide more useful build failures on other platforms (see [CB-7]).

[CB-7]: https://codeberg.org/flausch/gethostname.rs/issues/7

## [0.2.2] – 2022-01-14

## [0.2.1] – 2019-12-18
### Changed
- Consolidate documetation.
- Update crates.io metadata.

## [0.2.0] – 2019-01-22
### Added
- Add Windows implementation (see [GH-1]).

[GH-1]: https://github.com/swsnr/gethostname.rs/pull/1

### Changed
- Pin supported Rust version to 1.31

## 0.1.0 – 2019-01-20
Initial release.

### Added

- `gethostname()` for non-Windows platforms.

[Unreleased]: https://github.com/swsnr/gethostname.rs/compare/v0.4.3...HEAD
[0.4.3]: https://github.com/swsnr/gethostname.rs/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/swsnr/gethostname.rs/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/swsnr/gethostname.rs/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/swsnr/gethostname.rs/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/swsnr/gethostname.rs/compare/v0.2.3...v0.3.0
[0.2.3]: https://github.com/swsnr/gethostname.rs/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/swsnr/gethostname.rs/compare/gethostname-0.2.1...v0.2.2
[0.2.0]: https://github.com/swsnr/gethostname.rs/compare/gethostname-0.1.0...gethostname-0.2.0
[0.2.1]: https://github.com/swsnr/gethostname.rs/compare/gethostname-0.2.0...gethostname-0.2.1
