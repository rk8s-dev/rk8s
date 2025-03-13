use tokio::sync::Mutex;
use std::sync::Arc;

pub struct AtomicBool(Arc<Mutex<bool>>);
#[allow(unused)]
impl AtomicBool {
    // Create a new AtomicBool
    pub fn new(value: bool) -> Self {
        AtomicBool(Arc::new(Mutex::new(value)))
    }

    // Asynchronously load the current value
    pub async fn load(&self) -> bool {
        let lock = self.0.lock().await;
        *lock
    }

    // Asynchronously store a new value
    pub async fn store(&self, value: bool) {
        let mut lock = self.0.lock().await;
        *lock = value;
    }

    // Asynchronously compare and exchange
    pub async fn compare_exchange(&self, current: bool, new: bool) -> Result<bool, bool> {
        let mut lock = self.0.lock().await;
        if *lock == current {
            let old_value = *lock; // Read the current value
            *lock = new;           // Update to the new value
            Ok(old_value)          // Return the old value
        } else {
            Err(*lock)             // Return the current value
        }
    }
}

pub struct AtomicU32(Arc<Mutex<u32>>);
#[allow(unused)]
impl AtomicU32 {
    // Create a new AtomicU32
    pub fn new(value: u32) -> Self {
        AtomicU32(Arc::new(Mutex::new(value)))
    }

    // Asynchronously load the current value
    pub async fn load(&self) -> u32 {
        let lock = self.0.lock().await;
        *lock
    }

    // Asynchronously store a new value
    pub async fn store(&self, value: u32) {
        let mut lock = self.0.lock().await;
        *lock = value;
    }

    // Asynchronously fetch and add
    pub async fn fetch_add(&self, value: u32) -> u32 {
        let mut lock = self.0.lock().await;
        let old_value = *lock;  // Read the current value
        *lock += value;         // Add the specified value
        old_value               // Return the old value
    }

    // Asynchronously compare and exchange
    pub async fn compare_exchange(&self, current: u32, new: u32) -> Result<u32, u32> {
        let mut lock = self.0.lock().await;
        if *lock == current {
            let old_value = *lock; // Read the current value
            *lock = new;          // Update to the new value
            Ok(old_value)         // Return the old value
        } else {
            Err(*lock)            // Return the current value
        }
    }
}
#[derive(Debug)]
pub struct AtomicU64(Arc<Mutex<u64>>);
impl AtomicU64 {
    
    pub fn new(value: u64) -> Self {
        AtomicU64(Arc::new(Mutex::new(value)))
    }

    // async atom add 
    pub async fn fetch_add(&self, value: u64) -> u64 {
        let mut lock = self.0.lock().await; 
        *lock += value;                      
        *lock                           
    }
    pub async fn load(&self) -> u64 {
        let lock = self.0.lock().await;
        *lock
    }

    pub async fn compare_exchange(&self, current: u64, new: u64) -> std::result::Result<u64, u64> {
        let mut lock = self.0.lock().await; 
        if *lock == current {
            let old_value = *lock;          
            *lock = new;                   
            Ok(old_value)                 
        } else {
            Err(*lock)
        }
    }
}
