use crate::error::Result;
use crate::vec::VecExt;
use crate::{
    DeleteOptions, DeleteResponse, GetOptions, GetResponse, KvClient, PutOptions, PutResponse, Txn,
    TxnResponse,
};

pub struct KvClientPrefix {
    pfx: Vec<u8>,
    kv: KvClient,
}

impl KvClientPrefix {
    pub fn new(kv: KvClient, pfx: Vec<u8>) -> Self {
        Self { pfx, kv }
    }

    #[inline]
    fn prefixed_key(&self, key: impl Into<Vec<u8>>) -> Vec<u8> {
        let mut key = key.into();
        key.prefix_with(&self.pfx);
        key
    }

    pub async fn put(
        &mut self,
        key: impl Into<Vec<u8>>,
        value: impl Into<Vec<u8>>,
        options: Option<PutOptions>,
    ) -> Result<PutResponse> {
        let key = self.prefixed_key(key);
        let mut resp = self.kv.put(key, value, options).await?;
        resp.strip_prev_key_prefix(&self.pfx);
        Ok(resp)
    }

    pub async fn get(
        &mut self,
        key: impl Into<Vec<u8>>,
        mut options: Option<GetOptions>,
    ) -> Result<GetResponse> {
        let key = self.prefixed_key(key);
        options = options.map(|mut opts| {
            opts.key_range_end_mut().prefix_range_end_with(&self.pfx);
            opts
        });
        let mut resp = self.kv.get(key, options).await?;
        resp.strip_kvs_prefix(&self.pfx);
        Ok(resp)
    }

    pub async fn delete(
        &mut self,
        key: impl Into<Vec<u8>>,
        mut options: Option<DeleteOptions>,
    ) -> Result<DeleteResponse> {
        let key = self.prefixed_key(key);
        options = options.map(|mut opts| {
            opts.key_range_end_mut().prefix_range_end_with(&self.pfx);
            opts
        });
        let mut resp = self.kv.delete(key, options).await?;
        resp.strip_prev_kvs_prefix(&self.pfx);
        Ok(resp)
    }

    pub async fn txn(&mut self, mut txn: Txn) -> Result<TxnResponse> {
        txn.prefix_with(&self.pfx);
        let mut resp = self.kv.txn(txn).await?;
        resp.strip_key_prefix(&self.pfx);
        Ok(resp)
    }
}
