# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.1] - 2020-02-28

### Added

- `#![forbid(unsafe_code)]` attribute

## [1.0.0] - 2020-01-02

### Added

- `{}` formatting render colon-separated MAC address, e.g. `AB:0D:EF:12:34:56`
- `{:-}` formatting flag render hyphen-separated MAC address, e.g. `AB-0D-EF-12-34-56`
- `{:#}` formatting flag render period-separated MAC address, e.g. `AB0.DEF.123.456`

## [0.1.2] - 2019-10-17

### Added

- `MacAddr6::nil` and `MacAddr8::nil` methods to create new nil MAC addresses
- `MacAddr6::broadcast` and `MacAddr8::broadcast` methods to create new broadcast MAC addresses

### Fixed

- `std::fmt::Display` implementation for `MacAddr8` properly renders address in a canonical form (#1)
