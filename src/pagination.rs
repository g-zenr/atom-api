//! The shared pagination convention.
//!
//! Every list endpoint uses `limit` and `offset` query parameters and returns
//! the same envelope, so that clients can implement one pagination helper.
//! The convention is documented in `docs/api/conventions.md`.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Default number of items returned by a list endpoint.
pub const DEFAULT_LIMIT: i64 = 20;
/// Maximum number of items a caller may request.
pub const MAX_LIMIT: i64 = 100;

/// Query parameters accepted by every list endpoint.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PaginationQuery {
    /// Maximum number of items to return. Defaults to 20, capped at 100.
    #[serde(default)]
    pub limit: Option<i64>,
    /// Number of items to skip. Defaults to 0.
    #[serde(default)]
    pub offset: Option<i64>,
}

impl PaginationQuery {
    /// The effective limit, clamped into the allowed range.
    pub fn effective_limit(&self) -> i64 {
        match self.limit {
            Some(limit) if limit > 0 => limit.min(MAX_LIMIT),
            Some(_) => DEFAULT_LIMIT,
            None => DEFAULT_LIMIT,
        }
    }

    /// The effective offset; negative offsets are treated as zero.
    pub fn effective_offset(&self) -> i64 {
        match self.offset {
            Some(offset) if offset > 0 => offset,
            _ => 0,
        }
    }
}

/// The pagination block returned inside every list response.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Pagination {
    /// The limit that was applied.
    pub limit: i64,
    /// The offset that was applied.
    pub offset: i64,
    /// Total number of items matching the filter, ignoring pagination.
    pub total: i64,
    /// Whether more items are available after this page.
    pub has_more: bool,
}

impl Pagination {
    /// Builds the pagination block for a page.
    pub fn new(limit: i64, offset: i64, total: i64) -> Self {
        Self {
            limit,
            offset,
            total,
            has_more: offset + limit < total,
        }
    }
}

/// The envelope returned by every list endpoint.
///
/// The concrete OpenAPI schemas live next to each feature, because OpenAPI
/// needs a named schema per response type.
#[derive(Debug, Clone, Serialize)]
pub struct Page<T> {
    /// The items on this page.
    pub items: Vec<T>,
    /// Pagination metadata.
    pub pagination: Pagination,
}

impl<T> Page<T> {
    /// Builds a page envelope.
    pub fn new(items: Vec<T>, limit: i64, offset: i64, total: i64) -> Self {
        Self {
            items,
            pagination: Pagination::new(limit, offset, total),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_is_clamped_into_range() {
        let query = PaginationQuery {
            limit: Some(5_000),
            offset: None,
        };
        assert_eq!(query.effective_limit(), MAX_LIMIT);
    }

    #[test]
    fn non_positive_limit_falls_back_to_the_default() {
        let query = PaginationQuery {
            limit: Some(0),
            offset: Some(-3),
        };
        assert_eq!(query.effective_limit(), DEFAULT_LIMIT);
        assert_eq!(query.effective_offset(), 0);
    }

    #[test]
    fn has_more_reflects_the_remaining_items() {
        let pagination = Pagination::new(20, 0, 25);
        assert!(pagination.has_more);
        let pagination = Pagination::new(20, 20, 25);
        assert!(!pagination.has_more);
    }
}
