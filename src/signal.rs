use std::mem;
use std::ptr;
use std::sync::PoisonError;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Condvar;
use std::sync::Mutex;
use std::time::Duration;

use libc::c_int;
use libc::sigaction;
use libc::sighandler_t;

lazy_static::lazy_static! {
    pub(self) static ref CVAR: Condvar = Condvar::new();
    pub(self) static ref MUTEX: Mutex<bool> = Mutex::new(false);
}
pub(self) static MASK: AtomicU64 = AtomicU64::new(0);

pub(self) extern "C" fn handler(sig: c_int) {
    if sig <= 0 || sig >= 64 {
        return;
    }
    let mask = 0x1u64 << sig;
    MASK.fetch_or(mask, Ordering::Relaxed);
    CVAR.notify_one();
}


pub struct SignalsWaitError {
    
}

impl<Guard> From<PoisonError<Guard>> for SignalsWaitError {

    fn from(_e: PoisonError<Guard>) -> Self {
        SignalsWaitError {  }
    }
}

impl SignalsWaitError {

    pub fn unreachable() -> SignalsWaitError {
        SignalsWaitError {  }
    }
}


/// can only be called from main thread
pub(crate) unsafe fn register(signals: &[c_int]) {
    for &signum in signals {
        let mut action: sigaction = mem::zeroed();
        action.sa_sigaction = handler as extern "C" fn(_) as sighandler_t;
        action.sa_flags = libc::SA_NODEFER;
        if libc::sigaction(signum, &action, ptr::null_mut()) == 0 {
            
        }
    }
}


/// can only be called from main thread
pub(crate) unsafe fn wait(timeout: Duration) -> Result<c_int, SignalsWaitError> {

    {
        let mask = MASK.load(Ordering::Relaxed);
        let offset = mask.trailing_zeros();
        if offset < 32 {
            let m = 0x1u64 << offset;
            MASK.fetch_and(!m, Ordering::Relaxed);
            return Ok(offset as c_int);
        }
    }
    
    {
        let guard = MUTEX.lock()?;
        let (guard, result) = CVAR.wait_timeout(guard, timeout)?;
        if result.timed_out() {
            return Ok(0);
        }
    }
    
    {
        let mask = MASK.load(Ordering::Relaxed);
        let offset = mask.trailing_zeros();
        if offset < 32 {
            let m = 0x1u64 << offset;
            MASK.fetch_and(!m, Ordering::Relaxed);
            return Ok(offset as c_int);
        }
    }

    Err(SignalsWaitError::unreachable())
}
