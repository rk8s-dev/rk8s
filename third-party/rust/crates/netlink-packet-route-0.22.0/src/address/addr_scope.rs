// SPDX-License-Identifier: MIT

const RT_SCOPE_UNIVERSE: u8 = 0;
// 1 and 199 is user defined values
const RT_SCOPE_SITE: u8 = 200;
const RT_SCOPE_LINK: u8 = 253;
const RT_SCOPE_HOST: u8 = 254;
const RT_SCOPE_NOWHERE: u8 = 255;

#[derive(Clone, Eq, PartialEq, Debug, Copy, Default)]
#[non_exhaustive]
#[repr(u8)]
pub enum AddressScope {
    #[default]
    Universe,
    Site,
    Link,
    Host,
    Nowhere,
    Other(u8),
}

impl From<u8> for AddressScope {
    fn from(d: u8) -> Self {
        match d {
            RT_SCOPE_UNIVERSE => Self::Universe,
            RT_SCOPE_SITE => Self::Site,
            RT_SCOPE_LINK => Self::Link,
            RT_SCOPE_HOST => Self::Host,
            RT_SCOPE_NOWHERE => Self::Nowhere,
            _ => Self::Other(d),
        }
    }
}

impl From<AddressScope> for u8 {
    fn from(v: AddressScope) -> u8 {
        match v {
            AddressScope::Universe => RT_SCOPE_UNIVERSE,
            AddressScope::Site => RT_SCOPE_SITE,
            AddressScope::Link => RT_SCOPE_LINK,
            AddressScope::Host => RT_SCOPE_HOST,
            AddressScope::Nowhere => RT_SCOPE_NOWHERE,
            AddressScope::Other(d) => d,
        }
    }
}
