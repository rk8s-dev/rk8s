// Copyright 2016 coroutine-rs Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

extern crate context;

use context::{Context, Transfer};
use context::stack::ProtectedFixedSizeStack;

// Print the natural numbers from 0 to 9 using a "generator" preserving state on the stack.
fn main() {
    // This method will always `resume()` immediately back to the
    // previous `Context` with a `data` value incremented by one starting at 0.
    // You could thus describe this method as a "natural number generator".
    extern "C" fn context_function(mut t: Transfer) -> ! {
        for i in 0usize.. {
            print!("Yielding {} => ", i);
            t = unsafe { t.context.resume(i) };
        }

        unreachable!();
    }

    // Allocate some stack.
    let stack = ProtectedFixedSizeStack::default();

    // Allocate a Context on the stack.
    let mut t = Transfer::new(unsafe { Context::new(&stack, context_function) }, 0);

    // Yield 10 times to `context_function()`.
    for _ in 0..10 {
        // Yield to the "frozen" state of `context_function()`.
        // The `data` value is not used in this example and is left at 0.
        // The first and every other call will return references to the actual `Context` data.
        print!("Resuming => ");
        t = unsafe { t.context.resume(0) };

        println!("Got {}", t.data);
    }

    println!("Finished!");
}
