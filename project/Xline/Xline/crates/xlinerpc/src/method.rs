//! RPC method identifiers shared by QUIC transports.

macro_rules! define_method_ids {
    ( $( $(#[$meta:meta])* $variant:ident = $id:expr, $display:expr; )* ) => {
        /// Numeric RPC method identifier for QUIC transport.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u16)]
        pub enum MethodId {
            $( $(#[$meta])* $variant = $id, )*
        }

        impl MethodId {
            /// Decode from raw u16. Returns `None` for unknown IDs.
            #[inline]
            pub fn from_u16(v: u16) -> Option<Self> {
                match v {
                    $( $id => Some(Self::$variant), )*
                    _ => None,
                }
            }

            /// Encode to u16.
            #[inline]
            pub fn as_u16(self) -> u16 {
                self as u16
            }

            /// Human-readable name for logging/debugging.
            #[inline]
            pub fn name(self) -> &'static str {
                match self {
                    $( Self::$variant => $display, )*
                }
            }
        }

        /// All defined method IDs (for testing completeness).
        #[doc(hidden)]
        pub const ALL_METHOD_IDS: &[MethodId] = &[
            $( MethodId::$variant, )*
        ];
    };
}

define_method_ids! {
    // Protocol service (0x00xx)
    FetchCluster       = 0x0001, "FetchCluster";
    FetchReadState     = 0x0002, "FetchReadState";
    Record             = 0x0003, "Record";
    ReadIndex          = 0x0004, "ReadIndex";
    Shutdown           = 0x0005, "Shutdown";
    ProposeConfChange  = 0x0006, "ProposeConfChange";
    Publish            = 0x0007, "Publish";
    MoveLeader         = 0x0008, "MoveLeader";
    ProposeStream      = 0x0009, "ProposeStream";
    LeaseKeepAlive     = 0x000A, "LeaseKeepAlive";

    // InnerProtocol service (0x01xx)
    AppendEntries      = 0x0101, "AppendEntries";
    Vote               = 0x0102, "Vote";
    InstallSnapshot    = 0x0103, "InstallSnapshot";
    TriggerShutdown    = 0x0104, "TriggerShutdown";
    TryBecomeLeaderNow = 0x0105, "TryBecomeLeaderNow";

    // xline Auth direct-RPC service (0x02xx)
    XlineAuthenticate  = 0x0201, "XlineAuthenticate";

    // xline Lease direct-RPC service (0x03xx)
    XlineLeaseRevoke   = 0x0301, "XlineLeaseRevoke";
    XlineLeaseKeepAlive = 0x0302, "XlineLeaseKeepAlive";
    XlineLeaseTtl      = 0x0303, "XlineLeaseTtl";

    // xline Watch direct-RPC service (0x04xx)
    XlineWatch         = 0x0401, "XlineWatch";

    // xline Maintenance direct-RPC service (0x05xx)
    XlineSnapshot      = 0x0501, "XlineSnapshot";
    XlineAlarm         = 0x0502, "XlineAlarm";
    XlineMaintStatus   = 0x0503, "XlineMaintStatus";

    // xline Cluster direct-RPC service (0x06xx)
    XlineMemberAdd     = 0x0601, "XlineMemberAdd";
    XlineMemberRemove  = 0x0602, "XlineMemberRemove";
    XlineMemberPromote = 0x0603, "XlineMemberPromote";
    XlineMemberUpdate  = 0x0604, "XlineMemberUpdate";
    XlineMemberList    = 0x0605, "XlineMemberList";

    // xline KV direct-RPC service (0x07xx)
    XlineCompact       = 0x0701, "XlineCompact";
}
