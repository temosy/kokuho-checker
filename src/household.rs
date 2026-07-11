use serde::{Deserialize, Serialize};

/// A household as it appears on the premium notice. Includes the head of
/// household even when the head is not enrolled (擬制世帯主), because the
/// head's income counts toward the reduction qualification test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Household {
    pub members: Vec<Member>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    /// Age as of the fiscal year (used for the care-component 40-64 test).
    pub age: u8,
    /// Previous-year gross income (総所得金額等).
    pub gross_income_yen: u64,
    /// Enrolled in National Health Insurance. The head of household may be
    /// false here yet still counted for the reduction test.
    pub is_insured: bool,
    /// Salary or public-pension earner (給与所得者等) — affects the
    /// reduction qualification threshold.
    pub is_salary_or_pension_earner: bool,
    /// Preschool child (未就学児) — per-capita levy halved where supported.
    pub is_preschool: bool,
}

impl Household {
    pub fn insured(&self) -> impl Iterator<Item = &Member> {
        self.members.iter().filter(|m| m.is_insured)
    }

    pub fn insured_count(&self) -> u64 {
        self.insured().count() as u64
    }

    /// Total income used for the reduction qualification test. Sums all
    /// members including a non-insured head of household.
    ///
    /// Simplification: uses gross income as-is. The statutory test applies
    /// special treatment to elderly pension deductions and carried-forward
    /// losses; callers needing exact edge-case behavior should adjust
    /// `gross_income_yen` accordingly.
    pub fn reduction_test_income_yen(&self) -> u64 {
        self.members.iter().map(|m| m.gross_income_yen).sum()
    }

    pub fn earner_count(&self) -> u64 {
        self.members
            .iter()
            .filter(|m| m.is_salary_or_pension_earner)
            .count() as u64
    }
}
