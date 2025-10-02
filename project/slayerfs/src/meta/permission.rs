use bitflags::bitflags;
use sea_orm::{
    DbErr, QueryResult, TryGetError, TryGetable, Value,
    sea_query::{ValueType, ValueTypeErr},
};
use serde::{Deserialize, Serialize};

bitflags! {
    #[derive(Serialize, Deserialize)]
    pub struct AclFlags: u8 {
        const READ    = 0b001;
        const WRITE   = 0b010;
        const EXECUTE = 0b100;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AclSubject {
    User(u32),
    Group(u32),
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AclEntry {
    pub subject: AclSubject,
    pub flags: AclFlags,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Acl {
    pub entries: Vec<AclEntry>,
}

impl Acl {
    pub fn check_permission(&self, uid: u32, gids: &[u32], flag: AclFlags) -> bool {
        for entry in &self.entries {
            match &entry.subject {
                AclSubject::User(u) if *u == uid => {
                    if entry.flags.contains(flag) {
                        return true;
                    }
                }
                AclSubject::Group(g) if gids.contains(g) => {
                    if entry.flags.contains(flag) {
                        return true;
                    }
                }
                AclSubject::Other => {
                    if entry.flags.contains(flag) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permission {
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub acl: Option<Acl>,
}

impl Permission {
    pub fn new(mode: u32, uid: u32, gid: u32) -> Self {
        Self {
            mode,
            uid,
            gid,
            acl: None,
        }
    }
    pub fn permission_bits(&self) -> u32 {
        self.mode & 0o777
    }
    pub fn file_type_bits(&self) -> u32 {
        self.mode & !0o777
    }
    pub fn is_directory(&self) -> bool {
        self.file_type_bits() & 0o040000 == 0o040000
    }
    pub fn is_regular_file(&self) -> bool {
        self.file_type_bits() & 0o100000 == 0o100000
    }
    pub fn default_directory(uid: u32, gid: u32) -> Self {
        Self {
            mode: 0o040755,
            uid,
            gid,
            acl: None,
        }
    }
    pub fn default_file(uid: u32, gid: u32) -> Self {
        Self {
            mode: 0o100644,
            uid,
            gid,
            acl: None,
        }
    }
    pub fn check_access(
        &self,
        uid: u32,
        gids: &[u32],
        flag: AclFlags,
        ugo_mask: u32,
        group_mask: u32,
        other_mask: u32,
    ) -> bool {
        if uid == 0 {
            return true;
        }
        if let Some(acl) = &self.acl
            && acl.check_permission(uid, gids, flag)
        {
            return true;
        }
        let perm_bits = self.permission_bits();
        if uid == self.uid {
            return perm_bits & ugo_mask != 0;
        }
        if gids.contains(&self.gid) {
            return perm_bits & group_mask != 0;
        }
        perm_bits & other_mask != 0
    }
    pub fn can_read(&self, uid: u32, gids: &[u32]) -> bool {
        self.check_access(uid, gids, AclFlags::READ, 0o400, 0o040, 0o004)
    }
    pub fn can_write(&self, uid: u32, gids: &[u32]) -> bool {
        self.check_access(uid, gids, AclFlags::WRITE, 0o200, 0o020, 0o002)
    }
    pub fn can_execute(&self, uid: u32, gids: &[u32]) -> bool {
        self.check_access(uid, gids, AclFlags::EXECUTE, 0o100, 0o010, 0o001)
    }
    pub fn chmod(&mut self, new_mode: u32) {
        let file_type = self.file_type_bits();
        self.mode = file_type | (new_mode & 0o777);
    }
    pub fn chown(&mut self, new_uid: u32, new_gid: u32) {
        self.uid = new_uid;
        self.gid = new_gid;
    }
}

impl From<Permission> for Value {
    fn from(permission: Permission) -> Self {
        match serde_json::to_string(&permission) {
            Ok(json_str) => Value::String(Some(Box::new(json_str))),
            Err(_) => Value::String(None),
        }
    }
}

impl TryGetable for Permission {
    fn try_get_by<I: sea_orm::ColIdx>(res: &QueryResult, idx: I) -> Result<Self, TryGetError> {
        let json_str: String = res.try_get_by(idx)?;
        serde_json::from_str(&json_str).map_err(|e| {
            TryGetError::DbErr(DbErr::Type(format!(
                "Failed to deserialize Permission: {}",
                e
            )))
        })
    }
}

impl ValueType for Permission {
    fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
        match v {
            Value::String(Some(json_str)) => {
                serde_json::from_str(&json_str).map_err(|_| ValueTypeErr)
            }
            _ => Err(ValueTypeErr),
        }
    }

    fn type_name() -> String {
        "Permission".to_string()
    }

    fn array_type() -> sea_orm::sea_query::ArrayType {
        sea_orm::sea_query::ArrayType::String
    }

    fn column_type() -> sea_orm::sea_query::ColumnType {
        sea_orm::sea_query::ColumnType::Text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_creation() {
        let perm = Permission::new(0o755, 1000, 1000);
        assert_eq!(perm.mode, 0o755);
        assert_eq!(perm.uid, 1000);
        assert_eq!(perm.gid, 1000);
    }

    #[test]
    fn test_default_permissions() {
        let dir_perm = Permission::default_directory(1000, 1000);
        assert!(dir_perm.is_directory());
        assert_eq!(dir_perm.permission_bits(), 0o755);

        let file_perm = Permission::default_file(1000, 1000);
        assert!(file_perm.is_regular_file());
        assert_eq!(file_perm.permission_bits(), 0o644);
    }
}
