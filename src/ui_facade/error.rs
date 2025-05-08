// A dedicated error type for the facade for better diagnostics

use windows::core::Error as WinError;

#[derive(Debug)]
pub enum UiError {
    Win32(WinError),
    WindowCreationFailed,
    ClassRegistrationFailed,
    // Add more specific errors as the facade grows
}

// Implement From<windows::core::Error> for UiError for easy conversion
impl From<WinError> for UiError {
    fn from(err: WinError) -> Self {
        UiError::Win32(err)
    }
}

// Implement standard error traits
impl std::fmt::Display for UiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiError::Win32(e) => write!(f, "Win32 Error: {}", e),
            UiError::WindowCreationFailed => write!(f, "Window creation failed"),
            UiError::ClassRegistrationFailed => write!(f, "Window class registration failed"),
        }
    }
}

impl std::error::Error for UiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            UiError::Win32(e) => Some(e),
            _ => None,
        }
    }
}

// Define a Result type alias for convenience within the facade
pub type Result<T> = std::result::Result<T, UiError>;
