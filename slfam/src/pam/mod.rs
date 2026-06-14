//! # PAM Module
//!
//! Linux Pluggable Authentication Module integration for facial authentication.
//!
//! ## Usage
//!
//! Configure PAM to use this module by adding to `/etc/pam.d/common-auth`:
//!
//! ```text
//! auth sufficient pam_slfam.so
//! ```

mod conversation;
mod handler;

pub use handler::PamHandler;

use crate::config::Config;
use crate::error::{AuthError, PamError, Result};
use std::ffi::CStr;

/// PAM return codes
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PamResultCode {
    /// Successful authentication
    Success = 0,
    /// Authentication failure
    AuthError = 7,
    /// Insufficient credentials
    CredInsufficient = 8,
    /// Auth info unavailable
    AuthInfoUnavail = 9,
    /// User unknown
    UserUnknown = 10,
    /// Max tries exceeded
    MaxTries = 11,
    /// New auth token required
    NewAuthTokReqd = 12,
    /// Account expired
    AcctExpired = 13,
    /// Permission denied
    PermDenied = 6,
    /// Try again later
    TryAgain = 24,
    /// Ignore this module
    Ignore = 25,
    /// System error
    SystemErr = 4,
    /// Service error
    ServiceErr = 3,
}

impl From<PamResultCode> for i32 {
    fn from(code: PamResultCode) -> Self {
        code as i32
    }
}

impl From<&AuthError> for PamResultCode {
    fn from(err: &AuthError) -> Self {
        match err {
            AuthError::Camera(_) => PamResultCode::AuthInfoUnavail,
            AuthError::Detection(_) => PamResultCode::TryAgain,
            AuthError::Liveness(_) => PamResultCode::AuthError,
            AuthError::Embedding(_) => PamResultCode::TryAgain,
            AuthError::Crypto(_) => PamResultCode::SystemErr,
            AuthError::Template(_) => PamResultCode::UserUnknown,
            AuthError::Matching(_) => PamResultCode::AuthError,
            AuthError::Config(_) => PamResultCode::ServiceErr,
            AuthError::Pam(_) => PamResultCode::SystemErr,
            AuthError::RateLimited { .. } => PamResultCode::MaxTries,
            AuthError::Io(_) => PamResultCode::SystemErr,
            AuthError::AuthenticationFailed => PamResultCode::AuthError,
            AuthError::Internal(_) => PamResultCode::SystemErr,
        }
    }
}

/// PAM flags
#[derive(Debug, Clone, Copy)]
pub struct PamFlags {
    /// Silent operation
    pub silent: bool,
    /// Disallow null auth tokens
    pub disallow_null_authtok: bool,
    /// Try old auth token first
    pub try_first_pass: bool,
    /// Use old auth token
    pub use_first_pass: bool,
}

impl PamFlags {
    /// Parse flags from raw PAM flags integer
    pub fn from_raw(flags: i32) -> Self {
        Self {
            silent: (flags & 0x8000) != 0,
            disallow_null_authtok: (flags & 0x0001) != 0,
            try_first_pass: false, // Parsed from module args
            use_first_pass: false,
        }
    }
}

/// PAM module arguments
#[derive(Debug, Clone, Default)]
pub struct PamArgs {
    /// Configuration file path
    pub config_path: Option<String>,
    /// Enable debug logging
    pub debug: bool,
    /// Try password first
    pub try_first_pass: bool,
    /// Use password only
    pub use_first_pass: bool,
    /// Timeout in seconds
    pub timeout: Option<u32>,
    /// Skip liveness check (development only)
    pub skip_liveness: bool,
}

impl PamArgs {
    /// Parse arguments from PAM module args
    pub fn parse(argc: i32, argv: *const *const i8) -> Self {
        let mut args = Self::default();

        if argv.is_null() || argc <= 0 {
            return args;
        }

        for i in 0..argc as isize {
            let arg_ptr = unsafe { *argv.offset(i) };
            if arg_ptr.is_null() {
                continue;
            }

            let arg = unsafe { CStr::from_ptr(arg_ptr) }
                .to_string_lossy()
                .to_string();

            if arg.starts_with("conf=") {
                args.config_path = Some(arg[5..].to_string());
            } else if arg == "debug" {
                args.debug = true;
            } else if arg == "try_first_pass" {
                args.try_first_pass = true;
            } else if arg == "use_first_pass" {
                args.use_first_pass = true;
            } else if arg.starts_with("timeout=") {
                args.timeout = arg[8..].parse().ok();
            } else if arg == "skip_liveness" {
                args.skip_liveness = true;
            }
        }

        args
    }
}

/// Main PAM entry point for authentication
///
/// # Safety
///
/// This function is called by the PAM framework with raw pointers.
/// The caller must ensure:
/// - `pamh` is a valid PAM handle
/// - `argv` points to `argc` valid C strings (if argc > 0)
#[no_mangle]
pub unsafe extern "C" fn pam_sm_authenticate(
    pamh: *mut std::ffi::c_void,
    flags: i32,
    argc: i32,
    argv: *const *const i8,
) -> i32 {
    // Parse arguments
    let args = PamArgs::parse(argc, argv);
    let pam_flags = PamFlags::from_raw(flags);

    // Run authentication
    match authenticate_impl(pamh, pam_flags, args) {
        Ok(()) => PamResultCode::Success.into(),
        Err(e) => {
            // Log error if not silent
            if !pam_flags.silent {
                eprintln!("slfam: {}", e);
            }
            PamResultCode::from(&e).into()
        }
    }
}

/// Implementation of authentication logic
fn authenticate_impl(
    pamh: *mut std::ffi::c_void,
    _flags: PamFlags,
    args: PamArgs,
) -> Result<()> {
    // Load configuration
    let config_path = args.config_path.as_deref().unwrap_or("/etc/slfam/config.toml");
    let config = Config::load(config_path).map_err(|e| {
        AuthError::Config(crate::error::ConfigError::LoadFailed(e.to_string()))
    })?;

    // Get username from PAM
    let username = get_pam_user(pamh)?;

    // Create handler and authenticate
    let mut handler = PamHandler::new(config, args)?;
    handler.authenticate(&username)
}

/// Get username from PAM handle
fn get_pam_user(_pamh: *mut std::ffi::c_void) -> Result<String> {
    // In a real implementation, this would call pam_get_user
    // For now, return a placeholder
    
    // This would be:
    // let mut user_ptr: *const c_char = std::ptr::null();
    // let ret = pam_get_user(pamh, &mut user_ptr, std::ptr::null());
    // if ret != PAM_SUCCESS { return Err(...); }
    // Ok(CStr::from_ptr(user_ptr).to_string_lossy().to_string())
    
    Err(PamError::ConversationFailed("PAM bindings not implemented".to_string()).into())
}

/// PAM setcred entry point (required but not used for face auth)
///
/// # Safety
///
/// Called by PAM framework
#[no_mangle]
pub unsafe extern "C" fn pam_sm_setcred(
    _pamh: *mut std::ffi::c_void,
    _flags: i32,
    _argc: i32,
    _argv: *const *const i8,
) -> i32 {
    PamResultCode::Success.into()
}

/// PAM acct_mgmt entry point
///
/// # Safety
///
/// Called by PAM framework
#[no_mangle]
pub unsafe extern "C" fn pam_sm_acct_mgmt(
    _pamh: *mut std::ffi::c_void,
    _flags: i32,
    _argc: i32,
    _argv: *const *const i8,
) -> i32 {
    PamResultCode::Success.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pam_flags_parse() {
        let flags = PamFlags::from_raw(0x8000);
        assert!(flags.silent);
        assert!(!flags.disallow_null_authtok);
    }

    #[test]
    fn test_pam_result_code() {
        assert_eq!(i32::from(PamResultCode::Success), 0);
        assert_eq!(i32::from(PamResultCode::AuthError), 7);
    }

    #[test]
    fn test_pam_args_default() {
        let args = PamArgs::default();
        assert!(!args.debug);
        assert!(args.config_path.is_none());
    }
}
