use std::collections::VecDeque;

use serde::Serialize;
use ts_rs::TS;

#[derive(TS, Debug, Clone, Serialize, PartialEq)]
#[serde(into = "Vec<String>")]
#[ts(as = "Vec<String>")]
pub struct HashHistory {
    vec: VecDeque<String>,
    size: usize,
}

impl HashHistory {
    pub fn new(size: usize) -> Self {
        Self {
            vec: VecDeque::with_capacity(size),
            size,
        }
    }

    pub fn push(&mut self, item: String) {
        self.vec.push_front(item);
        if self.vec.len() > self.size {
            self.vec.pop_back();
        }
    }

    pub fn latest(&self) -> Option<&String> {
        self.vec.front()
    }

    #[allow(dead_code)] // todo
    pub fn contains(&self, item: impl AsRef<str>) -> bool {
        let item_str = item.as_ref();
        self.vec.iter().any(|s| s == item_str)
    }
}

impl From<HashHistory> for Vec<String> {
    fn from(hash_history: HashHistory) -> Self {
        hash_history.vec.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_history() {
        let mut hash_history = HashHistory::new(3);

        assert_eq!(hash_history.latest(), None);

        hash_history.push("1".to_string());
        hash_history.push("2".to_string());
        hash_history.push("3".to_string());
        assert!(hash_history.contains("1"));
        assert!(hash_history.contains("2"));
        assert!(hash_history.contains("3"));

        assert_eq!(hash_history.latest(), Some(&"3".to_string()));

        hash_history.push("4".to_string());
        assert!(!hash_history.contains("1"));
        assert!(hash_history.contains("2"));
        assert!(hash_history.contains("3"));
        assert!(hash_history.contains("4"));

        assert_eq!(hash_history.latest(), Some(&"4".to_string()));
    }
}
