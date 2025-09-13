// SPDX-License-Identifier: MIT

const ARPHRD_NETROM: u16 = 0;
const ARPHRD_ETHER: u16 = 1;
const ARPHRD_EETHER: u16 = 2;
const ARPHRD_AX25: u16 = 3;
const ARPHRD_PRONET: u16 = 4;
const ARPHRD_CHAOS: u16 = 5;
const ARPHRD_IEEE802: u16 = 6;
const ARPHRD_ARCNET: u16 = 7;
const ARPHRD_APPLETLK: u16 = 8;
const ARPHRD_DLCI: u16 = 15;
const ARPHRD_ATM: u16 = 19;
const ARPHRD_METRICOM: u16 = 23;
const ARPHRD_IEEE1394: u16 = 24;
const ARPHRD_EUI64: u16 = 27;
const ARPHRD_INFINIBAND: u16 = 32;
const ARPHRD_SLIP: u16 = 256;
const ARPHRD_CSLIP: u16 = 257;
const ARPHRD_SLIP6: u16 = 258;
const ARPHRD_CSLIP6: u16 = 259;
const ARPHRD_RSRVD: u16 = 260;
const ARPHRD_ADAPT: u16 = 264;
const ARPHRD_ROSE: u16 = 270;
const ARPHRD_X25: u16 = 271;
const ARPHRD_HWX25: u16 = 272;
const ARPHRD_CAN: u16 = 280;
const ARPHRD_PPP: u16 = 512;
const ARPHRD_CISCO: u16 = 513;
const ARPHRD_HDLC: u16 = ARPHRD_CISCO;
const ARPHRD_LAPB: u16 = 516;
const ARPHRD_DDCMP: u16 = 517;
const ARPHRD_RAWHDLC: u16 = 518;
const ARPHRD_RAWIP: u16 = 519;
const ARPHRD_TUNNEL: u16 = 768;
const ARPHRD_TUNNEL6: u16 = 769;
const ARPHRD_FRAD: u16 = 770;
const ARPHRD_SKIP: u16 = 771;
const ARPHRD_LOOPBACK: u16 = 772;
const ARPHRD_LOCALTLK: u16 = 773;
const ARPHRD_FDDI: u16 = 774;
const ARPHRD_BIF: u16 = 775;
const ARPHRD_SIT: u16 = 776;
const ARPHRD_IPDDP: u16 = 777;
const ARPHRD_IPGRE: u16 = 778;
const ARPHRD_PIMREG: u16 = 779;
const ARPHRD_HIPPI: u16 = 780;
const ARPHRD_ASH: u16 = 781;
const ARPHRD_ECONET: u16 = 782;
const ARPHRD_IRDA: u16 = 783;
const ARPHRD_FCPP: u16 = 784;
const ARPHRD_FCAL: u16 = 785;
const ARPHRD_FCPL: u16 = 786;
const ARPHRD_FCFABRIC: u16 = 787;
const ARPHRD_IEEE802_TR: u16 = 800;
const ARPHRD_IEEE80211: u16 = 801;
const ARPHRD_IEEE80211_PRISM: u16 = 802;
const ARPHRD_IEEE80211_RADIOTAP: u16 = 803;
const ARPHRD_IEEE802154: u16 = 804;
const ARPHRD_IEEE802154_MONITOR: u16 = 805;
const ARPHRD_PHONET: u16 = 820;
const ARPHRD_PHONET_PIPE: u16 = 821;
const ARPHRD_CAIF: u16 = 822;
const ARPHRD_IP6GRE: u16 = 823;
const ARPHRD_NETLINK: u16 = 824;
const ARPHRD_6LOWPAN: u16 = 825;
const ARPHRD_VSOCKMON: u16 = 826;
const ARPHRD_VOID: u16 = 0xffff;
const ARPHRD_NONE: u16 = 0xfffe;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
#[non_exhaustive]
#[repr(u16)]
// Since this list seldom changes, we do not add `Other(u16)` for unknown data.
// For unknown value, we log a warning message
pub enum LinkLayerType {
    #[default]
    Netrom = ARPHRD_NETROM,
    Ether = ARPHRD_ETHER,
    Eether = ARPHRD_EETHER,
    Ax25 = ARPHRD_AX25,
    Pronet = ARPHRD_PRONET,
    Chaos = ARPHRD_CHAOS,
    Ieee802 = ARPHRD_IEEE802,
    Arcnet = ARPHRD_ARCNET,
    Appletlk = ARPHRD_APPLETLK,
    Dlci = ARPHRD_DLCI,
    Atm = ARPHRD_ATM,
    Metricom = ARPHRD_METRICOM,
    Ieee1394 = ARPHRD_IEEE1394,
    Eui64 = ARPHRD_EUI64,
    Infiniband = ARPHRD_INFINIBAND,
    Slip = ARPHRD_SLIP,
    Cslip = ARPHRD_CSLIP,
    Slip6 = ARPHRD_SLIP6,
    Cslip6 = ARPHRD_CSLIP6,
    Rsrvd = ARPHRD_RSRVD,
    Adapt = ARPHRD_ADAPT,
    Rose = ARPHRD_ROSE,
    X25 = ARPHRD_X25,
    Hwx25 = ARPHRD_HWX25,
    Can = ARPHRD_CAN,
    Ppp = ARPHRD_PPP,
    Hdlc = ARPHRD_HDLC,
    Lapb = ARPHRD_LAPB,
    Ddcmp = ARPHRD_DDCMP,
    Rawhdlc = ARPHRD_RAWHDLC,
    Rawip = ARPHRD_RAWIP,
    Tunnel = ARPHRD_TUNNEL,
    Tunnel6 = ARPHRD_TUNNEL6,
    Frad = ARPHRD_FRAD,
    Skip = ARPHRD_SKIP,
    Loopback = ARPHRD_LOOPBACK,
    Localtlk = ARPHRD_LOCALTLK,
    Fddi = ARPHRD_FDDI,
    Bif = ARPHRD_BIF,
    Sit = ARPHRD_SIT,
    Ipddp = ARPHRD_IPDDP,
    Ipgre = ARPHRD_IPGRE,
    Pimreg = ARPHRD_PIMREG,
    Hippi = ARPHRD_HIPPI,
    Ash = ARPHRD_ASH,
    Econet = ARPHRD_ECONET,
    Irda = ARPHRD_IRDA,
    Fcpp = ARPHRD_FCPP,
    Fcal = ARPHRD_FCAL,
    Fcpl = ARPHRD_FCPL,
    Fcfabric = ARPHRD_FCFABRIC,
    Ieee802Tr = ARPHRD_IEEE802_TR,
    Ieee80211 = ARPHRD_IEEE80211,
    Ieee80211Prism = ARPHRD_IEEE80211_PRISM,
    Ieee80211Radiotap = ARPHRD_IEEE80211_RADIOTAP,
    Ieee802154 = ARPHRD_IEEE802154,
    Ieee802154Monitor = ARPHRD_IEEE802154_MONITOR,
    Phonet = ARPHRD_PHONET,
    PhonetPipe = ARPHRD_PHONET_PIPE,
    Caif = ARPHRD_CAIF,
    Ip6gre = ARPHRD_IP6GRE,
    Netlink = ARPHRD_NETLINK,
    Sixlowpan = ARPHRD_6LOWPAN,
    Vsockmon = ARPHRD_VSOCKMON,
    /// Void type, nothing is known
    Void = ARPHRD_VOID,
    /// zero header length
    None = ARPHRD_NONE,
}

impl From<u16> for LinkLayerType {
    fn from(d: u16) -> Self {
        match d {
            d if d == ARPHRD_NETROM => Self::Netrom,
            d if d == ARPHRD_ETHER => Self::Ether,
            d if d == ARPHRD_EETHER => Self::Eether,
            d if d == ARPHRD_AX25 => Self::Ax25,
            d if d == ARPHRD_PRONET => Self::Pronet,
            d if d == ARPHRD_CHAOS => Self::Chaos,
            d if d == ARPHRD_IEEE802 => Self::Ieee802,
            d if d == ARPHRD_ARCNET => Self::Arcnet,
            d if d == ARPHRD_APPLETLK => Self::Appletlk,
            d if d == ARPHRD_DLCI => Self::Dlci,
            d if d == ARPHRD_ATM => Self::Atm,
            d if d == ARPHRD_METRICOM => Self::Metricom,
            d if d == ARPHRD_IEEE1394 => Self::Ieee1394,
            d if d == ARPHRD_EUI64 => Self::Eui64,
            d if d == ARPHRD_INFINIBAND => Self::Infiniband,
            d if d == ARPHRD_SLIP => Self::Slip,
            d if d == ARPHRD_CSLIP => Self::Cslip,
            d if d == ARPHRD_SLIP6 => Self::Slip6,
            d if d == ARPHRD_CSLIP6 => Self::Cslip6,
            d if d == ARPHRD_RSRVD => Self::Rsrvd,
            d if d == ARPHRD_ADAPT => Self::Adapt,
            d if d == ARPHRD_ROSE => Self::Rose,
            d if d == ARPHRD_X25 => Self::X25,
            d if d == ARPHRD_HWX25 => Self::Hwx25,
            d if d == ARPHRD_CAN => Self::Can,
            d if d == ARPHRD_PPP => Self::Ppp,
            d if d == ARPHRD_HDLC => Self::Hdlc,
            d if d == ARPHRD_LAPB => Self::Lapb,
            d if d == ARPHRD_DDCMP => Self::Ddcmp,
            d if d == ARPHRD_RAWHDLC => Self::Rawhdlc,
            d if d == ARPHRD_RAWIP => Self::Rawip,
            d if d == ARPHRD_TUNNEL => Self::Tunnel,
            d if d == ARPHRD_TUNNEL6 => Self::Tunnel6,
            d if d == ARPHRD_FRAD => Self::Frad,
            d if d == ARPHRD_SKIP => Self::Skip,
            d if d == ARPHRD_LOOPBACK => Self::Loopback,
            d if d == ARPHRD_LOCALTLK => Self::Localtlk,
            d if d == ARPHRD_FDDI => Self::Fddi,
            d if d == ARPHRD_BIF => Self::Bif,
            d if d == ARPHRD_SIT => Self::Sit,
            d if d == ARPHRD_IPDDP => Self::Ipddp,
            d if d == ARPHRD_IPGRE => Self::Ipgre,
            d if d == ARPHRD_PIMREG => Self::Pimreg,
            d if d == ARPHRD_HIPPI => Self::Hippi,
            d if d == ARPHRD_ASH => Self::Ash,
            d if d == ARPHRD_ECONET => Self::Econet,
            d if d == ARPHRD_IRDA => Self::Irda,
            d if d == ARPHRD_FCPP => Self::Fcpp,
            d if d == ARPHRD_FCAL => Self::Fcal,
            d if d == ARPHRD_FCPL => Self::Fcpl,
            d if d == ARPHRD_FCFABRIC => Self::Fcfabric,
            d if d == ARPHRD_IEEE802_TR => Self::Ieee802Tr,
            d if d == ARPHRD_IEEE80211 => Self::Ieee80211,
            d if d == ARPHRD_IEEE80211_PRISM => Self::Ieee80211Prism,
            d if d == ARPHRD_IEEE80211_RADIOTAP => Self::Ieee80211Radiotap,
            d if d == ARPHRD_IEEE802154 => Self::Ieee802154,
            d if d == ARPHRD_IEEE802154_MONITOR => Self::Ieee802154Monitor,
            d if d == ARPHRD_PHONET => Self::Phonet,
            d if d == ARPHRD_PHONET_PIPE => Self::PhonetPipe,
            d if d == ARPHRD_CAIF => Self::Caif,
            d if d == ARPHRD_IP6GRE => Self::Ip6gre,
            d if d == ARPHRD_NETLINK => Self::Netlink,
            d if d == ARPHRD_6LOWPAN => Self::Sixlowpan,
            d if d == ARPHRD_VSOCKMON => Self::Vsockmon,
            d if d == ARPHRD_VOID => Self::Void,
            d if d == ARPHRD_NONE => Self::None,
            _ => {
                log::warn!(
                    "BUG: Got unknown ARPHRD_XXX {d} for LinkLayerType, \
                    treating it as LinkLayerType::Void"
                );
                Self::Void
            }
        }
    }
}

impl From<LinkLayerType> for u16 {
    fn from(v: LinkLayerType) -> u16 {
        v as u16
    }
}

impl std::fmt::Display for LinkLayerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Netrom => "NETROM",
                Self::Ether => "ETHER",
                Self::Eether => "EETHER",
                Self::Ax25 => "AX25",
                Self::Pronet => "PRONET",
                Self::Chaos => "CHAOS",
                Self::Ieee802 => "IEEE802",
                Self::Arcnet => "ARCNET",
                Self::Appletlk => "APPLETLK",
                Self::Dlci => "DLCI",
                Self::Atm => "ATM",
                Self::Metricom => "METRICOM",
                Self::Ieee1394 => "IEEE1394",
                Self::Eui64 => "EUI64",
                Self::Infiniband => "INFINIBAND",
                Self::Slip => "SLIP",
                Self::Cslip => "CSLIP",
                Self::Slip6 => "SLIP6",
                Self::Cslip6 => "CSLIP6",
                Self::Rsrvd => "RSRVD",
                Self::Adapt => "ADAPT",
                Self::Rose => "ROSE",
                Self::X25 => "X25",
                Self::Hwx25 => "HWX25",
                Self::Can => "CAN",
                Self::Ppp => "PPP",
                Self::Hdlc => "HDLC",
                Self::Lapb => "LAPB",
                Self::Ddcmp => "DDCMP",
                Self::Rawhdlc => "RAWHDLC",
                Self::Rawip => "RAWIP",
                Self::Tunnel => "TUNNEL",
                Self::Tunnel6 => "TUNNEL6",
                Self::Frad => "FRAD",
                Self::Skip => "SKIP",
                Self::Loopback => "LOOPBACK",
                Self::Localtlk => "LOCALTLK",
                Self::Fddi => "FDDI",
                Self::Bif => "BIF",
                Self::Sit => "SIT",
                Self::Ipddp => "IPDDP",
                Self::Ipgre => "IPGRE",
                Self::Pimreg => "PIMREG",
                Self::Hippi => "HIPPI",
                Self::Ash => "ASH",
                Self::Econet => "ECONET",
                Self::Irda => "IRDA",
                Self::Fcpp => "FCPP",
                Self::Fcal => "FCAL",
                Self::Fcpl => "FCPL",
                Self::Fcfabric => "FCFABRIC",
                Self::Ieee802Tr => "IEEE802_TR",
                Self::Ieee80211 => "IEEE80211",
                Self::Ieee80211Prism => "IEEE80211_PRISM",
                Self::Ieee80211Radiotap => "IEEE80211_RADIOTAP",
                Self::Ieee802154 => "IEEE802154",
                Self::Ieee802154Monitor => "IEEE802154_MONITOR",
                Self::Phonet => "PHONET",
                Self::PhonetPipe => "PHONET_PIPE",
                Self::Caif => "CAIF",
                Self::Ip6gre => "IP6GRE",
                Self::Netlink => "NETLINK",
                Self::Sixlowpan => "6LOWPAN",
                Self::Vsockmon => "VSOCKMON",
                Self::Void => "VOID",
                Self::None => "NONE",
            }
        )
    }
}

impl LinkLayerType {
    #[allow(non_upper_case_globals)]
    pub const Cisco: LinkLayerType = LinkLayerType::Hdlc;
}
