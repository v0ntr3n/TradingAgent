use std::collections::{HashMap, HashSet};

use crate::DataError;

#[derive(Clone, Debug, Default)]
pub struct VendorRouter {
    chains: HashMap<String, Vec<String>>,
}

impl VendorRouter {
    pub fn new<I, K, V>(entries: I) -> Result<Self, DataError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut chains = HashMap::new();

        for (category, raw_chain) in entries {
            let category = category.as_ref().trim();
            if category.is_empty() {
                return Err(DataError::InvalidConfig(
                    "vendor category cannot be empty".into(),
                ));
            }
            if chains.contains_key(category) {
                return Err(DataError::InvalidConfig(format!(
                    "duplicate vendor category: {category}"
                )));
            }

            let raw_chain = raw_chain.as_ref();
            if raw_chain.trim().is_empty() {
                return Err(DataError::InvalidConfig(format!(
                    "vendor chain for {category} cannot be empty"
                )));
            }

            let mut vendors = Vec::new();
            let mut seen = HashSet::new();
            for raw_vendor in raw_chain.split(',') {
                let vendor = raw_vendor.trim();
                if vendor.is_empty() {
                    return Err(DataError::InvalidConfig(format!(
                        "vendor chain for {category} contains an empty entry"
                    )));
                }
                if !seen.insert(vendor.to_owned()) {
                    return Err(DataError::InvalidConfig(format!(
                        "vendor chain for {category} contains duplicate vendor {vendor}"
                    )));
                }
                vendors.push(vendor.to_owned());
            }

            chains.insert(category.to_owned(), vendors);
        }

        Ok(Self { chains })
    }

    pub fn chain(&self, category: &str) -> Result<&[String], DataError> {
        self.chains
            .get(category)
            .map(Vec::as_slice)
            .ok_or_else(|| DataError::MissingChain(category.to_owned()))
    }
}
