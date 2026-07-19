// Drift fixture: a non-`pub` trait MUST NOT appear in the inventory
// (the extractor regex requires `pub trait`). Verifies row count = 1.

trait NonPubTrait {
    fn should_not_appear(&self);
}

pub struct DriftImpl;
impl NonPubTrait for DriftImpl {
    fn should_not_appear(&self) {}
}

pub trait DriftPubTrait {
    fn should_appear(&self) -> i32;
}
pub struct DriftImpl2;
impl DriftPubTrait for DriftImpl2 {
    fn should_appear(&self) -> i32 { 0 }
}
