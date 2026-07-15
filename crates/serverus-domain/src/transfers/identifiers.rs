use std::fmt;
use std::num::{NonZeroU128, NonZeroU32};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TransferId(NonZeroU128);

impl TransferId {
    pub const fn get(self) -> u128 {
        self.0.get()
    }
}

impl TryFrom<u128> for TransferId {
    type Error = InvalidTransferId;

    fn try_from(value: u128) -> Result<Self, Self::Error> {
        NonZeroU128::new(value).map(Self).ok_or(InvalidTransferId)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidTransferId;

impl fmt::Display for InvalidTransferId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a transfer ID must be non-zero")
    }
}

impl std::error::Error for InvalidTransferId {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryBudget(u16);

impl RetryBudget {
    pub const fn new(max_retries: u16) -> Self {
        Self(max_retries)
    }

    pub const fn max_retries(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttemptNumber(NonZeroU32);

impl AttemptNumber {
    pub const fn first() -> Self {
        Self(NonZeroU32::MIN)
    }

    pub const fn get(self) -> u32 {
        self.0.get()
    }

    pub(super) fn next(self) -> Self {
        let value = self
            .get()
            .checked_add(1)
            .expect("transfer attempt number overflowed");
        Self(NonZeroU32::new(value).expect("incremented attempt is non-zero"))
    }
}

impl TryFrom<u32> for AttemptNumber {
    type Error = InvalidAttemptNumber;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        NonZeroU32::new(value).map(Self).ok_or(InvalidAttemptNumber)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidAttemptNumber;

impl fmt::Display for InvalidAttemptNumber {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a transfer attempt number must be non-zero")
    }
}

impl std::error::Error for InvalidAttemptNumber {}
