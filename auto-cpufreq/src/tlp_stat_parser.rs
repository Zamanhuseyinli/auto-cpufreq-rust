// src/tlp_stat_parser.rs

use std::collections::HashMap;

pub struct TLPStatusParser {
    data: HashMap<String, String>,
}

impl TLPStatusParser {
    pub fn new(tlp_stat_output: &str) -> Self {
        let mut parser = Self {
            data: HashMap::new(),
        };
        parser.parse(tlp_stat_output);
        parser
    }

    fn parse(&mut self, data: &str) {
        for line in data.lines() {
            if let Some((key, val)) = line.split_once('=') {
                self.data.insert(
                    key.trim().to_lowercase(),
                    val.trim().to_string(),
                );
            }
        }
    }

    fn get_key(&self, key: &str) -> String {
        self.data.get(key).cloned().unwrap_or_default()
    }

    pub fn is_enabled(&self) -> bool {
        self.get_key("state") == "enabled"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tlp_parser() {
        let output = "state=enabled\nversion=1.3.1\nmode=battery";
        let parser = TLPStatusParser::new(output);
        
        assert!(parser.is_enabled());
        assert_eq!(parser.get_key("version"), "1.3.1");
        assert_eq!(parser.get_key("mode"), "battery");
    }

    #[test]
    fn test_tlp_parser_disabled() {
        let output = "state=disabled";
        let parser = TLPStatusParser::new(output);
        
        assert!(!parser.is_enabled());
    }

    #[test]
    fn test_tlp_parser_empty() {
        let parser = TLPStatusParser::new("");
        assert!(!parser.is_enabled());
    }
}
