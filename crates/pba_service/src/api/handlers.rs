pub mod normal;
pub mod pb;
pub mod transactions;

// Legacy re-exports so existing route definitions like `handlers::create_account`
// keep compiling during the migration. Task 2.16 will replace these with explicit
// module paths.
pub use pb::*;
pub use transactions::*;
