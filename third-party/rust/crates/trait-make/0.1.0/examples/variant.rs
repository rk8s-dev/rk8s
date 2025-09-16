// Copyright (c) 2023 Google LLC
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::{fmt::Display, future::Future};

#[trait_make::make(IntFactory: Send)]
pub trait LocalIntFactory {
    const NAME: &'static str;

    type MyFut<'a>: Future
    where
        Self: 'a;

    async fn make(&self, x: u32, y: &str) -> i32;
    fn stream(&self) -> impl Iterator<Item = i32>;
    fn call(&self) -> u32;
    fn another_async(&self, input: Result<(), &str>) -> Self::MyFut<'_>;
    fn default_stream(&self) -> impl Iterator<Item = i32> {
        [1].into_iter()
    }
    async fn default_method(&self, x: u32) -> u32 {
        x
    }
    fn sync_default_method(&self) -> u32 {
        1
    }
}

#[trait_make::make(Send)]
pub trait AnotherIntFactory {
    const NAME: &'static str;

    type MyFut<'a>: Future
    where
        Self: 'a;

    async fn make(&self, x: u32, y: &str) -> i32;
    fn stream(&self) -> impl Iterator<Item = i32>;
    fn call(&self) -> u32;
    fn another_async(&self, input: Result<(), &str>) -> Self::MyFut<'_>;
    fn default_stream(&self) -> impl Iterator<Item = i32> {
        [1].into_iter()
    }
    async fn default_method(&self, x: u32) -> u32 {
        x
    }
    fn sync_default_method(&self) -> u32 {
        1
    }
}

#[allow(dead_code)]
fn spawn_task(factory: impl IntFactory + 'static) {
    tokio::spawn(async move {
        let _int = factory.make(1, "foo").await;
        let _default_int = factory.default_method(1).await;
        let _default_stream = factory.default_stream();
        let _sync_default_int = factory.sync_default_method();
    });
}

#[allow(dead_code)]
fn spawn_another_task(factory: impl AnotherIntFactory + 'static) {
    tokio::spawn(async move {
        let _int = factory.make(1, "foo").await;
        let _default_int = factory.default_method(1).await;
        let _default_stream = factory.default_stream();
        let _sync_default_int = factory.sync_default_method();
    });
}

#[trait_make::make(GenericTrait: Send)]
pub trait LocalGenericTrait<'x, S: Sync, Y, const X: usize>
where
    Y: Sync,
{
    const CONST: usize = 3;
    type F;
    type A<const ANOTHER_CONST: u8>;
    type B<T: Display>: FromIterator<T>;

    async fn take(&self, s: S);
    fn build<T: Display>(&self, items: impl Iterator<Item = T>) -> Self::B<T>;
}

#[trait_make::make(Send + Sync)]
pub trait GenericTraitWithBounds<'x, S: Sync, Y, const X: usize>
where
    Y: Sync,
{
    const CONST: usize = 3;
    type F;
    type A<const ANOTHER_CONST: u8>;
    type B<T: Display>: FromIterator<T>;

    async fn take(&self, s: S);
    fn build<T: Display>(&self, items: impl Iterator<Item = T>) -> Self::B<T>;
}

fn main() {}
