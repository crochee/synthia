// Fixture for extract_trait_signals.sh self-test.
// Defines traits with KNOWN signal counts to verify the extractor.
//
// Expected signal counts (verified by running extract_trait_signals.sh
// on a workspace containing only this crate):
//
// | trait         | impl | methods | generics | lifetimes | assoc_types | call_sites | dyn | body_lines |
// |---------------|------|---------|----------|-----------|-------------|------------|-----|------------|
// | FixtureTraitA | 1    | 2       | 0        | 0         | 0           | 2          | 2   | 4          |
// | FixtureTraitB | 1    | 1       | 1        | 0         | 0           | 0          | 0   | 3          |

// Trait A: 1 impl, 2 methods, 0 generics, 0 lifetimes, 0 assoc_types,
//          2 dyn references (= 2 call_sites = 2 dyn), 0 plain `as` refs
pub trait FixtureTraitA {
    fn alpha(&self) -> i32;
    fn beta(&mut self, x: i32);
}

pub struct FixtureStructA;
impl FixtureTraitA for FixtureStructA {
    fn alpha(&self) -> i32 { 1 }
    fn beta(&mut self, _x: i32) {}
}

#[allow(dead_code)]
fn _use_a_as_trait() {
    // dyn reference #1
    let s = FixtureStructA;
    let _r: &dyn FixtureTraitA = &s;
}

#[allow(dead_code)]
fn _use_a_method() {
    // dyn reference #2 + method invocation
    let s = FixtureStructA;
    let r: &dyn FixtureTraitA = &s;
    let _v = r.alpha();
}

// Trait B: 1 impl, 1 method, 1 generic, 0 lifetimes, 0 assoc_types, 0 call sites, 0 dyn
pub trait FixtureTraitB<T: Clone> {
    fn transform(&self, input: T) -> T;
}

pub struct FixtureStructB;
impl<T: Clone + Default> FixtureTraitB<T> for FixtureStructB {
    fn transform(&self, input: T) -> T { input }
}
