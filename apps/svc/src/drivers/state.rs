//! What `drivers install` left on the machine, kept as registry values (spec 0014 → State).
//! Parsed strictly: anything unexpected is an error, never guessed around.

use std::collections::BTreeMap;

use crate::drivers::package::DRIVERS;
use crate::error::SvcError;

const CERT_VALUE: &str = "CertThumbprint";
const PUBLISHED_SUFFIX: &str = ".PublishedInf";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DriverState {
    /// SHA-1 thumbprint of the trusted signing certificate, 40 upper-case hex digits.
    pub cert_thumbprint: Option<String>,
    /// Driver name → published INF name in the driver store (`oemNN.inf`).
    pub published: BTreeMap<String, String>,
}

impl DriverState {
    pub fn is_empty(&self) -> bool {
        self.cert_thumbprint.is_none() && self.published.is_empty()
    }

    /// Registry value name / data pairs.
    pub fn to_values(&self) -> Vec<(String, String)> {
        let mut values = Vec::new();
        if let Some(thumb) = &self.cert_thumbprint {
            values.push((CERT_VALUE.to_owned(), thumb.clone()));
        }
        for (name, inf) in &self.published {
            values.push((format!("{name}{PUBLISHED_SUFFIX}"), inf.clone()));
        }
        values
    }

    pub fn from_values<I: IntoIterator<Item = (String, String)>>(
        values: I,
    ) -> Result<DriverState, SvcError> {
        let mut state = DriverState::default();
        for (name, data) in values {
            if name == CERT_VALUE {
                if !is_thumbprint(&data) {
                    return Err(SvcError::StateInvalid(format!("{name} = {data:?}")));
                }
                state.cert_thumbprint = Some(data);
            } else if let Some(driver) = name.strip_suffix(PUBLISHED_SUFFIX)
                && DRIVERS.iter().any(|d| d.name == driver)
                && is_published_inf(&data)
            {
                state.published.insert(driver.to_owned(), data);
            } else {
                return Err(SvcError::StateInvalid(format!("{name} = {data:?}")));
            }
        }
        Ok(state)
    }
}

fn is_thumbprint(s: &str) -> bool {
    s.len() == 40
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
}

/// `oem<digits>.inf`, any case.
fn is_published_inf(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    lower
        .strip_prefix("oem")
        .and_then(|rest| rest.strip_suffix(".inf"))
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
