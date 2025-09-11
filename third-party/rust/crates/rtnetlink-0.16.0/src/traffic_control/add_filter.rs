// SPDX-License-Identifier: MIT

use futures::stream::StreamExt;

use crate::{
    packet_core::{NetlinkMessage, NLM_F_ACK, NLM_F_REQUEST},
    packet_route::{
        tc::{
            TcAction, TcActionAttribute, TcActionGeneric, TcActionMirror,
            TcActionMirrorOption, TcActionOption, TcActionType, TcAttribute,
            TcFilterU32, TcFilterU32Option, TcHandle, TcHeader, TcMessage,
            TcMirror, TcMirrorActionType, TcOption, TcU32Key, TcU32Selector,
            TcU32SelectorFlags,
        },
        RouteNetlinkMessage,
    },
    try_nl, Error, Handle,
};

#[derive(Debug, Clone)]
pub struct TrafficFilterNewRequest {
    handle: Handle,
    message: TcMessage,
    flags: u16,
}

impl TrafficFilterNewRequest {
    pub(crate) fn new(handle: Handle, ifindex: i32, flags: u16) -> Self {
        Self {
            handle,
            message: TcMessage::with_index(ifindex),
            flags: NLM_F_REQUEST | flags,
        }
    }

    /// Execute the request
    pub async fn execute(self) -> Result<(), Error> {
        let Self {
            mut handle,
            message,
            flags,
        } = self;

        let mut req = NetlinkMessage::from(
            RouteNetlinkMessage::NewTrafficFilter(message),
        );
        req.header.flags = NLM_F_ACK | flags;

        let mut response = handle.request(req)?;
        while let Some(message) = response.next().await {
            try_nl!(message);
        }
        Ok(())
    }

    /// Set interface index.
    /// Equivalent to `dev STRING`, dev and block are mutually exlusive.
    pub fn index(mut self, index: i32) -> Self {
        self.message.header.index = index;
        self
    }

    /// Set block index.
    /// Equivalent to `block BLOCK_INDEX`.
    pub fn block(mut self, block_index: u32) -> Self {
        self.message.header.index = TcHeader::TCM_IFINDEX_MAGIC_BLOCK as i32;
        self.message.header.parent = block_index.into();
        self
    }

    /// Set parent.
    /// Equivalent to `[ root | ingress | egress | parent CLASSID ]`
    /// command args. They are mutually exclusive.
    pub fn parent(mut self, parent: u32) -> Self {
        self.message.header.parent = parent.into();
        self
    }

    /// Set parent to root.
    pub fn root(mut self) -> Self {
        self.message.header.parent = TcHandle::ROOT;
        self
    }

    /// Set parent to ingress.
    pub fn ingress(mut self) -> Self {
        self.message.header.parent = TcHandle {
            major: 0xffff,
            minor: TcHandle::MIN_INGRESS,
        };
        self
    }

    /// Set parent to egress.
    pub fn egress(mut self) -> Self {
        self.message.header.parent = TcHandle {
            major: 0xffff,
            minor: TcHandle::MIN_EGRESS,
        };
        self
    }

    /// Set priority.
    /// Equivalent to `priority PRIO` or `pref PRIO`.
    pub fn priority(mut self, priority: u16) -> Self {
        self.message.header.info = u32::from(TcHandle {
            major: priority,
            minor: priority,
        });
        self
    }

    /// Set protocol.
    /// Equivalent to `protocol PROT`.
    /// Default: ETH_P_ALL 0x0003, see llproto_names at iproute2/lib/ll_proto.c.
    pub fn protocol(mut self, protocol: u16) -> Self {
        self.message.header.info = u32::from(TcHandle {
            major: (self.message.header.info >> 16) as u16,
            minor: protocol,
        });
        self
    }

    /// The 32bit filter allows to match arbitrary bitfields in the packet.
    /// Equivalent to `tc filter ... u32`.
    pub fn u32(mut self, options: &[TcFilterU32Option]) -> Result<Self, Error> {
        if self
            .message
            .attributes
            .iter()
            .any(|nla| matches!(nla, TcAttribute::Kind(_)))
        {
            return Err(Error::InvalidNla(
                "message kind has already been set.".to_string(),
            ));
        }
        self.message
            .attributes
            .push(TcAttribute::Kind(TcFilterU32::KIND.to_string()));
        let mut nla_opts = Vec::new();
        for opt in options {
            nla_opts.push(TcOption::U32(opt.clone()));
        }
        self.message.attributes.push(TcAttribute::Options(nla_opts));
        Ok(self)
    }

    /// Use u32 to implement traffic redirect.
    /// Equivalent to
    /// `tc filter add [dev source] [parent ffff:] [protocol all] \
    ///     u32 match u8 0 0 action mirred egress redirect dev dest`
    /// You need to set the `parent` and `protocol` before call redirect.
    pub fn redirect(self, dst_index: u32) -> Result<Self, Error> {
        let mut sel_na = TcU32Selector::default();
        sel_na.flags = TcU32SelectorFlags::Terminal;
        sel_na.nkeys = 1;
        sel_na.keys = vec![TcU32Key::default()];
        let mut tc_mirror_nla = TcMirror::default();
        tc_mirror_nla.generic = TcActionGeneric::default();
        tc_mirror_nla.generic.action = TcActionType::Stolen;
        tc_mirror_nla.eaction = TcMirrorActionType::EgressRedir;
        tc_mirror_nla.ifindex = dst_index;
        let mut action_nla = TcAction::default();
        action_nla.attributes = vec![
            TcActionAttribute::Kind(TcActionMirror::KIND.to_string()),
            TcActionAttribute::Options(vec![TcActionOption::Mirror(
                TcActionMirrorOption::Parms(tc_mirror_nla),
            )]),
        ];
        let u32_nla = vec![
            TcFilterU32Option::Selector(sel_na),
            TcFilterU32Option::Action(vec![action_nla]),
        ];
        self.u32(&u32_nla)
    }
}

#[cfg(test)]
mod test {
    use std::{fs::File, os::fd::AsFd, path::Path};

    use futures::stream::TryStreamExt;
    use nix::sched::{setns, CloneFlags};
    use tokio::runtime::Runtime;

    use crate::{
        new_connection,
        packet_route::{
            link::LinkMessage,
            tc::{
                TcAttribute, TcFilterU32, TcFilterU32Option, TcOption,
                TcU32Key, TcU32SelectorFlags,
            },
        },
        Handle, LinkVeth, NetworkNamespace, NETNS_PATH, SELF_NS_PATH,
    };

    const TEST_NS: &str = "netlink_test_filter_ns";
    const TEST_VETH_1: &str = "test_veth_1";
    const TEST_VETH_2: &str = "test_veth_2";

    struct Netns {
        path: String,
        _cur: File,
        last: File,
    }

    impl Netns {
        async fn new(path: &str) -> Self {
            // record current ns
            let last = File::open(Path::new(SELF_NS_PATH)).unwrap();

            // create new ns
            NetworkNamespace::add(path.to_string()).await.unwrap();

            // entry new ns
            let ns_path = Path::new(NETNS_PATH);
            let file = File::open(ns_path.join(path)).unwrap();
            setns(file.as_fd(), CloneFlags::CLONE_NEWNET).unwrap();

            Self {
                path: path.to_string(),
                _cur: file,
                last,
            }
        }
    }
    impl Drop for Netns {
        fn drop(&mut self) {
            println!("exit ns: {}", self.path);
            setns(self.last.as_fd(), CloneFlags::CLONE_NEWNET).unwrap();

            let ns_path = Path::new(NETNS_PATH).join(&self.path);
            nix::mount::umount2(&ns_path, nix::mount::MntFlags::MNT_DETACH)
                .unwrap();
            nix::unistd::unlink(&ns_path).unwrap();
            // _cur File will be closed auto
            // Since there is no async drop, NetworkNamespace::del cannot be
            // called here. Dummy interface will be deleted
            // automatically after netns is deleted.
        }
    }

    async fn setup_env() -> (Handle, LinkMessage, LinkMessage, Netns) {
        let netns = Netns::new(TEST_NS).await;

        // Notice: The Handle can only be created after the setns, so that the
        // Handle is the connection within the new ns.
        let (connection, handle, _) = new_connection().unwrap();
        tokio::spawn(connection);
        handle
            .link()
            .add(LinkVeth::new(TEST_VETH_1, TEST_VETH_2).build())
            .execute()
            .await
            .unwrap();

        let mut links = handle
            .link()
            .get()
            .match_name(TEST_VETH_1.to_string())
            .execute();
        let link1 = links.try_next().await.unwrap();
        links = handle
            .link()
            .get()
            .match_name(TEST_VETH_2.to_string())
            .execute();
        let link2 = links.try_next().await.unwrap();
        (handle, link1.unwrap(), link2.unwrap(), netns)
    }

    async fn test_async_new_filter() {
        let (handle, test1, test2, _netns) = setup_env().await;
        handle
            .qdisc()
            .add(test1.header.index as i32)
            .ingress()
            .execute()
            .await
            .unwrap();

        handle
            .qdisc()
            .add(test2.header.index as i32)
            .ingress()
            .execute()
            .await
            .unwrap();

        handle
            .traffic_filter(test1.header.index as i32)
            .add()
            .parent(0xffff0000)
            .protocol(0x0003)
            .redirect(test2.header.index)
            .unwrap()
            .execute()
            .await
            .unwrap();

        // Verify that attempting to set 2 redirects causes and error
        assert!(handle
            .traffic_filter(test1.header.index as i32)
            .add()
            .parent(0xffff0000)
            .protocol(0x0003)
            .redirect(test2.header.index)
            .unwrap()
            .redirect(test1.header.index)
            .is_err());

        let mut filters_iter = handle
            .traffic_filter(test1.header.index as i32)
            .get()
            .root()
            .execute();

        let mut found = false;
        while let Some(nl_msg) = filters_iter.try_next().await.unwrap() {
            //filters.push(nl_msg.clone());
            if nl_msg.header.handle == 0x80000800.into() {
                let mut iter = nl_msg.attributes.iter();
                assert_eq!(
                    iter.next().unwrap(),
                    &TcAttribute::Kind(TcFilterU32::KIND.to_string()),
                );
                assert!(matches!(iter.next().unwrap(), &TcAttribute::Chain(_)));
                // TCA_OPTIONS
                let nla = iter.next().unwrap();
                let filter = if let TcAttribute::Options(f) = nla {
                    f
                } else {
                    panic!("expect options nla");
                };
                let mut fi = filter.iter();
                let fa = fi.next().unwrap();
                let ua = if let TcOption::U32(u) = fa {
                    u
                } else {
                    panic!("expect u32 nla");
                };
                // TCA_U32_SEL
                let sel = if let TcFilterU32Option::Selector(s) = ua {
                    s
                } else {
                    panic!("expect sel nla");
                };
                assert_eq!(sel.flags, TcU32SelectorFlags::Terminal);
                assert_eq!(sel.nkeys, 1);
                assert_eq!(sel.keys.len(), 1);
                assert_eq!(sel.keys[0], TcU32Key::default());
                found = true;
                break;
            }
        }
        if !found {
            panic!("not found :{} filter.", test1.header.index);
        }
    }

    #[test]
    fn test_new_filter() {
        Runtime::new().unwrap().block_on(test_async_new_filter());
    }
}
