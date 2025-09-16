// SPDX-License-Identifier: MIT

use futures::stream::StreamExt;

use crate::{
    packet_core::{NetlinkMessage, NLM_F_ACK, NLM_F_REQUEST},
    packet_route::{
        tc::{TcAttribute, TcHandle, TcMessage},
        RouteNetlinkMessage,
    },
    try_nl, Error, Handle,
};

#[derive(Debug, Clone)]
pub struct QDiscNewRequest {
    handle: Handle,
    message: TcMessage,
    flags: u16,
}

impl QDiscNewRequest {
    pub(crate) fn new(handle: Handle, message: TcMessage, flags: u16) -> Self {
        Self {
            handle,
            message,
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
            RouteNetlinkMessage::NewQueueDiscipline(message),
        );
        req.header.flags = NLM_F_ACK | flags;

        let mut response = handle.request(req)?;
        while let Some(message) = response.next().await {
            try_nl!(message);
        }
        Ok(())
    }

    /// Set handle,
    pub fn handle(mut self, major: u16, minor: u16) -> Self {
        self.message.header.handle = TcHandle { major, minor };
        self
    }

    /// Set parent to root.
    pub fn root(mut self) -> Self {
        self.message.header.parent = TcHandle::ROOT;
        self
    }

    /// Set parent
    pub fn parent(mut self, parent: u32) -> Self {
        self.message.header.parent = parent.into();
        self
    }

    /// New a ingress qdisc
    pub fn ingress(mut self) -> Self {
        self.message.header.parent = TcHandle::INGRESS;
        self.message.header.handle = TcHandle::from(0xffff0000);
        self.message
            .attributes
            .push(TcAttribute::Kind("ingress".to_string()));
        self
    }
}

#[cfg(test)]
mod test {
    use std::{fs::File, os::fd::AsFd, path::Path};

    use futures::stream::TryStreamExt;
    use nix::sched::{setns, CloneFlags};
    use tokio::runtime::Runtime;

    use super::*;
    use crate::{
        new_connection,
        packet_route::{link::LinkMessage, AddressFamily},
        LinkDummy, NetworkNamespace, NETNS_PATH, SELF_NS_PATH,
    };

    const TEST_NS: &str = "netlink_test_qdisc_ns";
    const TEST_DUMMY: &str = "test_dummy";

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

    async fn setup_env() -> (Handle, LinkMessage, Netns) {
        let netns = Netns::new(TEST_NS).await;

        // Notice: The Handle can only be created after the setns, so that the
        // Handle is the connection within the new ns.
        let (connection, handle, _) = new_connection().unwrap();
        tokio::spawn(connection);
        handle
            .link()
            .add(LinkDummy::new(TEST_DUMMY).up().build())
            .execute()
            .await
            .unwrap();
        let mut links = handle
            .link()
            .get()
            .match_name(TEST_DUMMY.to_string())
            .execute();
        let link = links.try_next().await.unwrap();
        (handle, link.unwrap(), netns)
    }

    async fn test_async_new_qdisc() {
        let (handle, test_link, _netns) = setup_env().await;
        handle
            .qdisc()
            .add(test_link.header.index as i32)
            .ingress()
            .execute()
            .await
            .unwrap();
        let mut qdiscs_iter = handle
            .qdisc()
            .get()
            .index(test_link.header.index as i32)
            .ingress()
            .execute();

        let mut found = false;
        while let Some(nl_msg) = qdiscs_iter.try_next().await.unwrap() {
            if nl_msg.header.index == test_link.header.index as i32
                && nl_msg.header.handle == 0xffff0000.into()
            {
                assert_eq!(nl_msg.header.family, AddressFamily::Unspec);
                assert_eq!(nl_msg.header.handle, 0xffff0000.into());
                assert_eq!(nl_msg.header.parent, TcHandle::INGRESS);
                assert_eq!(nl_msg.header.info, 1); // refcount
                assert_eq!(
                    nl_msg.attributes[0],
                    TcAttribute::Kind("ingress".to_string())
                );
                assert_eq!(nl_msg.attributes[2], TcAttribute::HwOffload(0));
                found = true;
                break;
            }
        }
        if !found {
            panic!("not found dev:{} qdisc.", test_link.header.index);
        }
    }

    #[test]
    fn test_new_qdisc() {
        Runtime::new().unwrap().block_on(test_async_new_qdisc());
    }
}
