pub mod error;

use std::fmt::Display;

use serde::{Deserialize, Serialize};

use super::primitives::{ChartId, DebitOrCredit};
use error::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Hash, Deserialize)]
pub struct AccountIdx(u64);
impl Display for AccountIdx {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}
impl From<u32> for AccountIdx {
    fn from(num: u32) -> Self {
        Self(num.into())
    }
}

impl AccountIdx {
    pub const FIRST: Self = Self(1);
    pub const MAX_TWO_DIGIT: Self = Self(99);
    pub const MAX_THREE_DIGIT: Self = Self(999);

    pub const fn next(&self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartCategoryPath {
    Assets = 1,
    Liabilities = 2,
    Equity = 3,
    Revenues = 4,
    Expenses = 5,
}

impl Display for ChartCategoryPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Assets => write!(f, "Assets"),
            Self::Liabilities => write!(f, "Liabilities"),
            Self::Equity => write!(f, "Equity"),
            Self::Revenues => write!(f, "Revenues"),
            Self::Expenses => write!(f, "Expenses"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartPath {
    Category(ChartCategoryPath),
    ControlAccount {
        category: ChartCategoryPath,
        index: AccountIdx,
    },
    ControlSubAccount {
        category: ChartCategoryPath,
        control_index: AccountIdx,
        index: AccountIdx,
    },
    TransactionAccount {
        category: ChartCategoryPath,
        control_index: AccountIdx,
        control_sub_index: AccountIdx,
        index: AccountIdx,
    },
}

impl std::fmt::Display for ChartPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Category(category) => {
                write!(f, "{:01}000000", *category as u32)
            }
            Self::ControlAccount { category, index } => {
                write!(f, "{:01}{:02}0000", *category as u32, index)
            }
            Self::ControlSubAccount {
                category,
                control_index,
                index,
            } => {
                write!(
                    f,
                    "{:01}{:02}{:02}000",
                    *category as u32, control_index, index
                )
            }
            Self::TransactionAccount {
                category,
                control_index,
                control_sub_index,
                index,
            } => {
                write!(
                    f,
                    "{:01}{:02}{:02}{:03}",
                    *category as u32, control_index, control_sub_index, index
                )
            }
        }
    }
}

impl std::str::FromStr for ChartPath {
    type Err = ChartPathError;

    fn from_str(s: &str) -> Result<Self, ChartPathError> {
        if s.len() != 8 {
            return Err(ChartPathError::InvalidCodeLength(s.to_string()));
        }

        fn parse_segment(s: &str) -> Result<u32, ChartPathError> {
            Ok(s.parse::<u32>()?)
        }

        let category_segment = parse_segment(&s[0..1])?;
        let category = Self::category_from_number(category_segment)
            .ok_or(ChartPathError::InvalidCategoryNumber(category_segment))?;

        let control = parse_segment(&s[1..3])?;
        let sub = parse_segment(&s[3..5])?;
        let trans = parse_segment(&s[5..8])?;

        match (control, sub, trans) {
            (0, 0, 0) => Ok(Self::Category(category)),
            (c, 0, 0) if c > 0 => Ok(Self::ControlAccount {
                category,
                index: c.into(),
            }),
            (c, s, 0) if c > 0 && s > 0 => Ok(Self::ControlSubAccount {
                category,
                control_index: c.into(),
                index: s.into(),
            }),
            (c, s, t) if c > 0 && s > 0 && t > 0 => Ok(Self::TransactionAccount {
                category,
                control_index: c.into(),
                control_sub_index: s.into(),
                index: t.into(),
            }),
            _ => Err(ChartPathError::InvalidCodeString(s.to_string())),
        }
    }
}

impl ChartPath {
    fn category_from_number(num: u32) -> Option<ChartCategoryPath> {
        match num {
            1 => Some(ChartCategoryPath::Assets),
            2 => Some(ChartCategoryPath::Liabilities),
            3 => Some(ChartCategoryPath::Equity),
            4 => Some(ChartCategoryPath::Revenues),
            5 => Some(ChartCategoryPath::Expenses),
            _ => None,
        }
    }

    pub fn normal_balance_type(&self) -> DebitOrCredit {
        match self.category() {
            ChartCategoryPath::Assets | ChartCategoryPath::Expenses => DebitOrCredit::Debit,
            _ => DebitOrCredit::Credit,
        }
    }

    pub fn to_code(&self, chart_id: ChartId) -> String {
        format!("{}::{}", chart_id, self)
    }

    pub fn category(&self) -> ChartCategoryPath {
        match *self {
            Self::Category(category) => category,
            Self::ControlAccount { category, .. } => category,
            Self::ControlSubAccount { category, .. } => category,
            Self::TransactionAccount { category, .. } => category,
        }
    }

    pub fn control_account(&self) -> Option<ChartPath> {
        match *self {
            Self::ControlAccount { category, index } => {
                Some(Self::ControlAccount { category, index })
            }
            Self::ControlSubAccount {
                category,
                control_index,
                ..
            } => Some(Self::ControlAccount {
                category,
                index: control_index,
            }),
            Self::TransactionAccount {
                category,
                control_index,
                ..
            } => Some(Self::ControlAccount {
                category,
                index: control_index,
            }),
            Self::Category(_) => None,
        }
    }

    pub fn control_sub_account(&self) -> Option<ChartPath> {
        match *self {
            Self::TransactionAccount {
                category,
                control_index,
                control_sub_index,
                ..
            } => Some(Self::ControlSubAccount {
                category,
                control_index,
                index: control_sub_index,
            }),
            Self::ControlSubAccount {
                category,
                control_index,
                index,
            } => Some(Self::ControlSubAccount {
                category,
                control_index,
                index,
            }),
            _ => None,
        }
    }

    pub const fn first_control_account(category: ChartPath) -> Result<Self, ChartPathError> {
        match category {
            Self::Category(category) => Ok(Self::ControlAccount {
                category,
                index: AccountIdx::FIRST,
            }),
            _ => Err(ChartPathError::InvalidCategoryPathForNewControlAccount),
        }
    }

    pub fn first_control_sub_account(control_account: &Self) -> Result<Self, ChartPathError> {
        match control_account {
            Self::ControlAccount { category, index } => Ok(Self::ControlSubAccount {
                category: *category,
                control_index: *index,
                index: AccountIdx::FIRST,
            }),
            _ => Err(ChartPathError::InvalidControlAccountPathForNewControlSubAccount),
        }
    }

    pub fn first_transaction_account(control_sub_account: &Self) -> Result<Self, ChartPathError> {
        match control_sub_account {
            Self::ControlSubAccount {
                category,
                control_index,
                index,
            } => Ok(Self::TransactionAccount {
                category: *category,
                control_index: *control_index,
                control_sub_index: *index,
                index: AccountIdx::FIRST,
            }),
            _ => Err(ChartPathError::InvalidSubControlAccountPathForNewTransactionAccount),
        }
    }

    pub fn next(&self) -> Result<Self, ChartPathError> {
        match *self {
            Self::Category(_) => Ok(*self), // Categories don't have next
            Self::ControlAccount { category, index } => {
                let next_index = index.next();
                if next_index > AccountIdx::MAX_TWO_DIGIT {
                    Err(ChartPathError::ControlIndexOverflowForCategory(category))
                } else {
                    Ok(Self::ControlAccount {
                        category,
                        index: next_index,
                    })
                }
            }
            Self::ControlSubAccount {
                category,
                control_index,
                index,
            } => {
                let next_index = index.next();
                if next_index > AccountIdx::MAX_TWO_DIGIT {
                    Err(ChartPathError::ControlSubIndexOverflowForControlAccount(
                        category,
                        control_index,
                    ))
                } else {
                    Ok(Self::ControlSubAccount {
                        category,
                        control_index,
                        index: next_index,
                    })
                }
            }
            Self::TransactionAccount {
                category,
                control_index,
                control_sub_index,
                index,
            } => {
                let next_index = index.next();
                if next_index > AccountIdx::MAX_THREE_DIGIT {
                    Err(
                        ChartPathError::TransactionIndexOverflowForControlSubAccount(
                            category,
                            control_index,
                            control_sub_index,
                        ),
                    )
                } else {
                    Ok(Self::TransactionAccount {
                        category,
                        control_index,
                        control_sub_index,
                        index: next_index,
                    })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    mod convert_to_string {
        use super::*;

        #[test]
        fn test_category_formatting() {
            let code = ChartPath::Category(ChartCategoryPath::Assets);
            assert_eq!(code.to_string(), "1000000");
        }

        #[test]
        fn test_control_account_formatting() {
            let code = ChartPath::ControlAccount {
                category: ChartCategoryPath::Liabilities,
                index: 1.into(),
            };
            assert_eq!(code.to_string(), "2010000");
        }

        #[test]
        fn test_control_sub_account_formatting() {
            let code = ChartPath::ControlSubAccount {
                category: ChartCategoryPath::Equity,
                control_index: 1.into(),
                index: 2.into(),
            };
            assert_eq!(code.to_string(), "30102000");
        }

        #[test]
        fn test_transaction_account_formatting() {
            let code = ChartPath::TransactionAccount {
                category: ChartCategoryPath::Revenues,
                control_index: 1.into(),
                control_sub_index: 2.into(),
                index: 3.into(),
            };
            assert_eq!(code.to_string(), "40102003");
        }
    }

    mod parse_code_from_string {
        use super::*;

        #[test]
        fn test_parsing_valid_codes() {
            assert_eq!(
                ChartPath::from_str("10000000").unwrap(),
                ChartPath::Category(ChartCategoryPath::Assets)
            );

            assert_eq!(
                ChartPath::from_str("20100000").unwrap(),
                ChartPath::ControlAccount {
                    category: ChartCategoryPath::Liabilities,
                    index: 1.into(),
                }
            );

            assert_eq!(
                ChartPath::from_str("30102000").unwrap(),
                ChartPath::ControlSubAccount {
                    category: ChartCategoryPath::Equity,
                    control_index: 1.into(),
                    index: 2.into(),
                }
            );

            assert_eq!(
                ChartPath::from_str("40102003").unwrap(),
                ChartPath::TransactionAccount {
                    category: ChartCategoryPath::Revenues,
                    control_index: 1.into(),
                    control_sub_index: 2.into(),
                    index: 3.into(),
                }
            );
        }

        #[test]
        fn test_invalid_code_length() {
            match ChartPath::from_str("100") {
                Err(ChartPathError::InvalidCodeLength(code)) => {
                    assert_eq!(code, "100");
                }
                other => panic!("Expected InvalidCodeLength error, got {:?}", other),
            }
        }

        #[test]
        fn test_invalid_category() {
            match ChartPath::from_str("90000000") {
                Err(ChartPathError::InvalidCategoryNumber(num)) => {
                    assert_eq!(num, 9);
                }
                other => panic!("Expected InvalidCategoryNumber error, got {:?}", other),
            }
        }

        #[test]
        fn test_invalid_code_format() {
            match ChartPath::from_str("10002030") {
                Err(ChartPathError::InvalidCodeString(code)) => {
                    assert_eq!(code, "10002030");
                }
                other => panic!("Expected InvalidCodeString error, got {:?}", other),
            }
        }

        #[test]
        fn test_non_numeric_input() {
            match ChartPath::from_str("A0000000") {
                Err(ChartPathError::ParseIntError(_)) => {
                    // ParseIntError doesn't implement PartialEq, so we just check the variant
                }
                other => panic!("Expected ParseIntError, got {:?}", other),
            }
        }
    }

    mod category_extraction_tests {
        use super::*;

        #[test]
        fn test_category_from_category_code() {
            for category in [
                ChartCategoryPath::Assets,
                ChartCategoryPath::Liabilities,
                ChartCategoryPath::Equity,
                ChartCategoryPath::Revenues,
                ChartCategoryPath::Expenses,
            ] {
                let code = ChartPath::Category(category);
                assert_eq!(code.category(), category);
            }
        }

        #[test]
        fn test_category_from_control_account() {
            for category in [
                ChartCategoryPath::Assets,
                ChartCategoryPath::Liabilities,
                ChartCategoryPath::Equity,
                ChartCategoryPath::Revenues,
                ChartCategoryPath::Expenses,
            ] {
                let code = ChartPath::ControlAccount {
                    category,
                    index: 1.into(),
                };
                assert_eq!(code.category(), category);
            }
        }

        #[test]
        fn test_category_from_control_sub_account() {
            for category in [
                ChartCategoryPath::Assets,
                ChartCategoryPath::Liabilities,
                ChartCategoryPath::Equity,
                ChartCategoryPath::Revenues,
                ChartCategoryPath::Expenses,
            ] {
                let code = ChartPath::ControlSubAccount {
                    category,
                    control_index: 1.into(),
                    index: 2.into(),
                };
                assert_eq!(code.category(), category);
            }
        }

        #[test]
        fn test_category_from_transaction_account() {
            for category in [
                ChartCategoryPath::Assets,
                ChartCategoryPath::Liabilities,
                ChartCategoryPath::Equity,
                ChartCategoryPath::Revenues,
                ChartCategoryPath::Expenses,
            ] {
                let code = ChartPath::TransactionAccount {
                    category,
                    control_index: 1.into(),
                    control_sub_index: 2.into(),
                    index: 3.into(),
                };
                assert_eq!(code.category(), category);
            }
        }
    }

    mod control_account_extraction_tests {
        use super::*;

        const CATEGORY: ChartCategoryPath = ChartCategoryPath::Assets;
        const CONTROL_INDEX: AccountIdx = AccountIdx::FIRST;
        const EXPECTED: ChartPath = ChartPath::ControlAccount {
            category: CATEGORY,
            index: CONTROL_INDEX,
        };

        #[test]
        fn test_control_account_from_transaction_account() {
            let transaction = ChartPath::TransactionAccount {
                category: CATEGORY,
                control_index: CONTROL_INDEX,
                control_sub_index: 2.into(),
                index: 3.into(),
            };

            assert_eq!(transaction.control_account(), Some(EXPECTED));
        }

        #[test]
        fn test_control_account_from_control_sub_account() {
            let sub_account = ChartPath::ControlSubAccount {
                category: CATEGORY,
                control_index: CONTROL_INDEX,
                index: 2.into(),
            };

            assert_eq!(sub_account.control_account(), Some(EXPECTED));
        }

        #[test]
        fn test_control_account_from_control_account() {
            let control_account = ChartPath::ControlAccount {
                category: CATEGORY,
                index: CONTROL_INDEX,
            };

            assert_eq!(control_account.control_account(), Some(EXPECTED));
        }

        #[test]
        fn test_control_account_from_category_returns_none() {
            let category_code = ChartPath::Category(CATEGORY);
            assert_eq!(category_code.control_account(), None);
        }
    }

    mod control_sub_account_extraction_tests {
        use super::*;

        const CATEGORY: ChartCategoryPath = ChartCategoryPath::Assets;
        const CONTROL_INDEX: AccountIdx = AccountIdx::FIRST;
        const SUB_INDEX: AccountIdx = AccountIdx::FIRST;
        const EXPECTED: ChartPath = ChartPath::ControlSubAccount {
            category: CATEGORY,
            control_index: CONTROL_INDEX,
            index: SUB_INDEX,
        };

        #[test]
        fn test_control_sub_account_from_transaction_account() {
            let transaction = ChartPath::TransactionAccount {
                category: CATEGORY,
                control_index: CONTROL_INDEX,
                control_sub_index: SUB_INDEX,
                index: 3.into(),
            };

            assert_eq!(transaction.control_sub_account(), Some(EXPECTED));
        }

        #[test]
        fn test_control_sub_account_from_control_sub_account() {
            let sub_account = ChartPath::ControlSubAccount {
                category: CATEGORY,
                control_index: CONTROL_INDEX,
                index: SUB_INDEX,
            };

            assert_eq!(sub_account.control_sub_account(), Some(EXPECTED));
        }

        #[test]
        fn test_control_sub_account_from_control_account_returns_none() {
            let control_account = ChartPath::ControlAccount {
                category: CATEGORY,
                index: CONTROL_INDEX,
            };

            assert_eq!(control_account.control_sub_account(), None);
        }

        #[test]
        fn test_control_sub_account_from_category_returns_none() {
            let category_code = ChartPath::Category(CATEGORY);
            assert_eq!(category_code.control_sub_account(), None);
        }
    }

    mod first_account_create {
        use super::*;

        #[test]
        fn test_first_control_account_creation() {
            let category = ChartPath::Category(ChartCategoryPath::Assets);
            let control = ChartPath::first_control_account(category).unwrap();

            assert_eq!(
                control,
                ChartPath::ControlAccount {
                    category: ChartCategoryPath::Assets,
                    index: AccountIdx::FIRST,
                }
            );
        }

        #[test]
        fn test_first_control_account_invalid_input() {
            let invalid_input = ChartPath::ControlAccount {
                category: ChartCategoryPath::Assets,
                index: 1.into(),
            };

            assert!(ChartPath::first_control_account(invalid_input).is_err());
        }

        #[test]
        fn test_first_control_sub_account_creation() {
            let control = ChartPath::ControlAccount {
                category: ChartCategoryPath::Assets,
                index: AccountIdx::FIRST,
            };

            let sub = ChartPath::first_control_sub_account(&control).unwrap();
            assert_eq!(
                sub,
                ChartPath::ControlSubAccount {
                    category: ChartCategoryPath::Assets,
                    control_index: AccountIdx::FIRST,
                    index: AccountIdx::FIRST,
                }
            );
        }

        #[test]
        fn test_first_control_sub_account_invalid_input() {
            let invalid_input = ChartPath::Category(ChartCategoryPath::Assets);
            assert!(ChartPath::first_control_sub_account(&invalid_input).is_err());
        }

        #[test]
        fn test_first_transaction_account_creation() {
            let sub = ChartPath::ControlSubAccount {
                category: ChartCategoryPath::Assets,
                control_index: AccountIdx::FIRST,
                index: AccountIdx::FIRST,
            };

            let transaction = ChartPath::first_transaction_account(&sub).unwrap();
            assert_eq!(
                transaction,
                ChartPath::TransactionAccount {
                    category: ChartCategoryPath::Assets,
                    control_index: AccountIdx::FIRST,
                    control_sub_index: AccountIdx::FIRST,
                    index: AccountIdx::FIRST,
                }
            );
        }

        #[test]
        fn test_first_transaction_account_invalid_input() {
            let invalid_input = ChartPath::Category(ChartCategoryPath::Assets);
            assert!(ChartPath::first_transaction_account(&invalid_input).is_err());
        }
    }

    mod next_account_create {
        use super::*;

        #[test]
        fn test_next_control_account_success() {
            let control = ChartPath::ControlAccount {
                category: ChartCategoryPath::Assets,
                index: 1.into(),
            };

            let next_control = control.next().unwrap();
            assert_eq!(
                next_control,
                ChartPath::ControlAccount {
                    category: ChartCategoryPath::Assets,
                    index: 2.into(),
                }
            );
        }

        #[test]
        fn test_next_control_account_overflow() {
            let max_control = ChartPath::ControlAccount {
                category: ChartCategoryPath::Assets,
                index: AccountIdx::MAX_TWO_DIGIT,
            };
            assert!(max_control.next().is_err());
        }

        #[test]
        fn test_next_control_sub_account_success() {
            let sub = ChartPath::ControlSubAccount {
                category: ChartCategoryPath::Assets,
                control_index: 1.into(),
                index: 1.into(),
            };

            let next_sub = sub.next().unwrap();
            assert_eq!(
                next_sub,
                ChartPath::ControlSubAccount {
                    category: ChartCategoryPath::Assets,
                    control_index: 1.into(),
                    index: 2.into(),
                }
            );
        }

        #[test]
        fn test_next_control_sub_account_overflow() {
            let max_sub = ChartPath::ControlSubAccount {
                category: ChartCategoryPath::Assets,
                control_index: 1.into(),
                index: AccountIdx::MAX_TWO_DIGIT,
            };
            assert!(max_sub.next().is_err());
        }

        #[test]
        fn test_next_transaction_account_success() {
            let transaction = ChartPath::TransactionAccount {
                category: ChartCategoryPath::Assets,
                control_index: 1.into(),
                control_sub_index: 1.into(),
                index: 1.into(),
            };

            let next_transaction = transaction.next().unwrap();
            assert_eq!(
                next_transaction,
                ChartPath::TransactionAccount {
                    category: ChartCategoryPath::Assets,
                    control_index: 1.into(),
                    control_sub_index: 1.into(),
                    index: 2.into(),
                }
            );
        }

        #[test]
        fn test_next_transaction_account_overflow() {
            let max_transaction = ChartPath::TransactionAccount {
                category: ChartCategoryPath::Assets,
                control_index: 1.into(),
                control_sub_index: 1.into(),
                index: AccountIdx::MAX_THREE_DIGIT,
            };
            assert!(max_transaction.next().is_err());
        }

        #[test]
        fn test_next_category_returns_same() {
            let category = ChartPath::Category(ChartCategoryPath::Assets);
            let next_category = category.next().unwrap();
            assert_eq!(category, next_category);
        }
    }
}
