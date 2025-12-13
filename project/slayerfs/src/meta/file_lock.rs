use sea_orm::{
    TryGetError, Value,
    sea_query::{self, ValueTypeErr},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum FileLockType {
    ReadLock = libc::F_RDLCK as u32,
    WriteLock = libc::F_WRLCK as u32,
    UnLock = libc::F_UNLCK as u32,
}

impl FileLockType {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            x if x == Self::ReadLock as u32 => Some(Self::ReadLock),
            x if x == Self::WriteLock as u32 => Some(Self::WriteLock),
            x if x == Self::UnLock as u32 => Some(Self::UnLock),
            _ => None,
        }
    }

    pub fn as_u32(&self) -> u32 {
        *self as u32
    }
}

impl std::convert::From<FileLockType> for sea_orm::Value {
    fn from(value: FileLockType) -> Self {
        match value {
            FileLockType::ReadLock => Value::Unsigned(Some(FileLockType::ReadLock as u32)),
            FileLockType::WriteLock => Value::Unsigned(Some(FileLockType::WriteLock as u32)),
            FileLockType::UnLock => Value::Unsigned(Some(FileLockType::UnLock as u32)),
        }
    }
}

impl sea_orm::TryGetable for FileLockType {
    fn try_get_by<I: sea_orm::ColIdx>(
        res: &sea_orm::QueryResult,
        index: I,
    ) -> Result<Self, sea_orm::TryGetError> {
        let val: u32 = res.try_get_by(index)?;
        FileLockType::from_u32(val).ok_or(TryGetError::DbErr(sea_orm::DbErr::Type(
            "Failed to deserialize FIleLockType".to_string(),
        )))
    }
}

impl sea_query::ValueType for FileLockType {
    fn try_from(v: Value) -> Result<Self, sea_query::ValueTypeErr> {
        match v {
            Value::Unsigned(Some(val)) => FileLockType::from_u32(val).ok_or(ValueTypeErr),
            _ => Err(sea_query::ValueTypeErr),
        }
    }

    fn type_name() -> String {
        "FlockType".to_string()
    }

    fn array_type() -> sea_query::ArrayType {
        sea_orm::sea_query::ArrayType::Unsigned
    }

    fn column_type() -> sea_orm::ColumnType {
        sea_orm::sea_query::ColumnType::Unsigned
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlockRecord {
    pub lock_type: FileLockType,
    pub pid: u32,
    pub lock_range: FileLockRange,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct FileLockRange {
    pub start: u64,
    pub end: u64,
}

impl FileLockRange {
    pub fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        self.end >= other.start && self.start <= other.end
    }
}
#[derive(Debug, Clone, Copy)]
pub struct FileLockQuery {
    pub owner: u64,
    pub lock_type: FileLockType,
    pub range: FileLockRange,
}

#[derive(Debug, Clone, Copy)]
pub struct FileLockInfo {
    pub lock_type: FileLockType,
    pub range: FileLockRange,
    pub pid: u32,
}

impl FileLockInfo {
    pub fn unlocked() -> Self {
        Self {
            lock_type: FileLockType::UnLock,
            range: FileLockRange::default(),
            pid: 0,
        }
    }
}

pub async fn get_plock(
    range: FileLockRange,
    query: FileLockQuery,
    lock_owner: i64,
    records: Vec<PlockRecord>,
) -> Option<FileLockInfo> {
    for record in records {
        // Check if this lock overlaps with the requested range
        if record.lock_range.overlaps(&range) {
            // Check if the lock conflicts with the query
            // Same owner can access its own locks
            if lock_owner == query.owner as i64 {
                return Some(FileLockInfo {
                    lock_type: record.lock_type,
                    range: record.lock_range,
                    pid: record.pid,
                });
            }

            // Check compatibility based on lock types
            match (record.lock_type, query.lock_type) {
                (FileLockType::ReadLock, FileLockType::ReadLock) => {
                    // Read locks are compatible
                    continue;
                }
                (FileLockType::UnLock, _) => {
                    // Unlocked region
                    continue;
                }
                _ => {
                    // Conflict detected
                    return Some(FileLockInfo {
                        lock_type: record.lock_type,
                        range: record.lock_range,
                        pid: record.pid,
                    });
                }
            }
        }
    }
    None
}

pub async fn check_conflicts(
    owner: u64,
    block: bool,
    lock_type: FileLockType,
    range: FileLockRange,
    lock_owner: i64,
    records: Vec<PlockRecord>,
) -> bool {
    for record in records {
        if record.lock_range.overlaps(&range) {
            // skip if same owner (allow re-locking or upgrading)
            if lock_owner == owner as i64 {
                continue;
            }

            // check lock compatibility
            match (record.lock_type, lock_type) {
                (FileLockType::ReadLock, FileLockType::ReadLock) => {
                    // read locks are compatible
                    continue;
                }
                _ => {
                    // conflict detected
                    if !block {
                        return true;
                    }

                    // for blocking locks, we would implement retry logic here
                    // for now, just return conflict error
                    return true;
                }
            }
        }
    }
    false
}
