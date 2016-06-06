//! High layer providing somewhat automatic marshalling of Rust closures
//! as C function pointers.
//!
//! The main facility here is given by the structs `Closure`*`N`* and
//! `ClosureMut`*`N`*, for natural numbers *`N`* from 0 to 12 (as of now).
//! These represent C closures, which can be used to turn Rust lambdas
//! (or in generally, anything that implements `Fn` or `FnMut`) into
//! ordinary C function pointers. For example, a Rust value of type
//! `Fn(u32, u32) -> u64` can be turned into a closure of type
//! `Closure2<u32, u32, u64>`. Then a C function pointer of type
//! `extern "C" fn(u32, u32) -> u64` can be borrowed from the closure
//! and passed to C.
//!
//! Here’s an example using [`ClosureMut1`](struct.ClosureMut1.html):
//!
//! ```
//! use libffi::high::ClosureMut1;
//!
//! let mut x = 0u64;
//! let mut f = |y: u32| { x += y as u64; x };
//!
//! let closure = ClosureMut1::new(&mut f);
//! let counter = closure.code_ptr();
//!
//! assert_eq!(5, counter(5));
//! assert_eq!(6, counter(1));
//! assert_eq!(8, counter(2));
//! ```
//!
//! Note that in the above example, `closure` is an ordinary C function
//! pointer of type `extern "C" fn(u64) -> u64`.
use std::marker::PhantomData;
use std::{mem, ptr};

use middle;

/// Representations of C types for the high layer.
pub mod types;
use self::types::*;

macro_rules! define_closure_types {
    (
        $cif:ident $callback:ident $callback_mut:ident
                   $closure:ident  $closure_mut:ident ;
                   $( $T:ident )*
    )
        =>
    {
        /// Typed CIF (“Call InterFace”), which statically tracks
        /// argument and result types.
        pub struct $cif<$( $T, )* R> {
            untyped: middle::Cif,
            _marker: PhantomData<fn($( $T, )*) -> R>,
        }

        impl<$( $T, )* R> $cif<$( $T, )* R> {
            /// Creates a new statically-typed CIF from the given argument
            /// and result types.
            #[allow(non_snake_case)]
            pub fn new($( $T: Type<$T>, )* result: Type<R>) -> Self {
                let cif = middle::Cif::new(vec![$( $T.into_untyped() ),*],
                                           result.into_untyped());
                $cif { untyped: cif, _marker: PhantomData }
            }
        }

        impl<$( $T: FfiType, )* R: FfiType> $cif<$( $T, )* R> {
            /// Creates a new statically-typed CIF by reifying the
            /// argument types as `Type<T>`s.
            pub fn reify() -> Self {
                Self::new($( $T::get_type(), )* R::get_type())
            }
        }

        // We use tuples of pointers to describe the arguments, and we
        // extract them by pattern matching. This assumes that a tuple
        // of pointers will be laid out packed and in order. This seems
        // to hold true right now, and I can’t think of a reason why it
        // wouldn’t be that way, but technically it may be undefined
        // behavior.

        /// The type of function that gets called from an immutable
        /// typed closure.
        pub type $callback<U, $( $T, )* R>
            = extern "C" fn(cif:      &::low::ffi_cif,
                            result:   &mut R,
                            args:     &($( &$T, )*),
                            userdata: &U);

        /// The type of function that gets called from a mutable
        /// typed closure.
        pub type $callback_mut<U, $( $T, )* R>
            = extern "C" fn(cif:      &::low::ffi_cif,
                            result:   &mut R,
                            args:     &($( &$T, )*),
                            userdata: &mut U);

        /// A mutable typed closure with the given argument and result
        /// types.
        pub struct $closure<'a, $( $T, )* R> {
            untyped: middle::Closure<'a>,
            _marker: PhantomData<fn($( $T, )*) -> R>,
        }

        impl<'a, $($T: Clone + FfiType,)* R: FfiType>
            $closure<'a, $($T,)* R>
        {
            /// Constructs a typed closure callable from C from a
            /// Rust closure.
            pub fn new<Callback>(callback: &'a Callback) -> Self
                where Callback: Fn($( $T, )*) -> R + 'a
            {
                Self::new_with_cif($cif::reify(), callback)
            }
        }

        impl<'a, $( $T, )* R> $closure<'a, $( $T, )* R> {
            /// Gets the C code pointer that is used to invoke the
            /// closure.
            pub fn code_ptr(&self) -> &extern "C" fn($( $T, )*) -> R {
                unsafe {
                    mem::transmute(self.untyped.code_ptr())
                }
            }

            /// Constructs a typed closure callable from C from a CIF
            /// describing the calling convention for the resulting
            /// function, a callback for the function to call, and
            /// userdata to pass to the callback.
            pub fn from_parts<U>(cif: $cif<$( $T, )* R>,
                                 callback: $callback<U, $( $T, )* R>,
                                 userdata: &'a U) -> Self
            {
                let callback: middle::Callback<U, R>
                    = unsafe { mem::transmute(callback) };
                let closure
                    = middle::Closure::new(cif.untyped,
                                           callback,
                                           userdata);
                $closure {
                    untyped: closure,
                    _marker: PhantomData,
                }
            }
        }

        impl<'a, $( $T: Clone, )* R> $closure<'a, $( $T, )* R> {
            /// Constructs a typed closure callable from C from a CIF
            /// describing the calling convention for the resulting
            /// function and the Rust closure to call.
            pub fn new_with_cif<Callback>(cif: $cif<$( $T, )* R>,
                                          callback: &'a Callback) -> Self
                where Callback: Fn($( $T, )*) -> R + 'a
            {
                Self::from_parts(cif,
                                 Self::static_callback,
                                 callback)
            }

            #[allow(non_snake_case)]
            extern "C" fn static_callback<Callback>
                (_cif:     &::low::ffi_cif,
                 result:   &mut R,
                 &($( $T, )*):
                           &($( &$T, )*),
                 userdata: &Callback)
              where Callback: Fn($( $T, )*) -> R + 'a
            {
                unsafe {
                    ptr::write(result, userdata($( $T.clone(), )*));
                }
            }
        }

        /// An immutable typed closure with the given argument and
        /// result types.
        pub struct $closure_mut<'a, $( $T, )* R> {
            untyped: middle::Closure<'a>,
            _marker: PhantomData<fn($( $T, )*) -> R>,
        }

        impl<'a, $($T: Clone + FfiType,)* R: FfiType>
            $closure_mut<'a, $($T,)* R>
        {
            /// Constructs a typed closure callable from C from a
            /// Rust closure.
            pub fn new<Callback>(callback: &'a mut Callback) -> Self
                where Callback: FnMut($( $T, )*) -> R + 'a
            {
                Self::new_with_cif($cif::reify(), callback)
            }
        }

        impl<'a, $( $T, )* R> $closure_mut<'a, $( $T, )* R> {
            /// Gets the C code pointer that is used to invoke the
            /// closure.
            pub fn code_ptr(&self) -> &extern "C" fn($( $T, )*) -> R {
                unsafe {
                    mem::transmute(self.untyped.code_ptr())
                }
            }

            /// Constructs a typed closure callable from C from a CIF
            /// describing the calling convention for the resulting
            /// function, a callback for the function to call, and
            /// userdata to pass to the callback.
            pub fn from_parts<U>(cif:      $cif<$( $T, )* R>,
                                 callback: $callback_mut<U, $( $T, )* R>,
                                 userdata: &'a mut U) -> Self
            {
                let callback: middle::CallbackMut<U, R>
                    = unsafe { mem::transmute(callback) };
                let closure
                    = middle::Closure::new_mut(cif.untyped,
                                               callback,
                                               userdata);
                $closure_mut {
                    untyped: closure,
                    _marker: PhantomData,
                }
            }
        }

        impl<'a, $( $T: Clone, )* R> $closure_mut<'a, $( $T, )* R> {
            /// Constructs a typed closure callable from C from a CIF
            /// describing the calling convention for the resulting
            /// function and the Rust closure to call.
            pub fn new_with_cif<Callback>(cif: $cif<$( $T, )* R>,
                                          callback: &'a mut Callback) -> Self
                where Callback: FnMut($( $T, )*) -> R + 'a
            {
                Self::from_parts(cif,
                                 Self::static_callback,
                                 callback)
            }

            #[allow(non_snake_case)]
            extern "C" fn static_callback<Callback>
                (_cif:     &::low::ffi_cif,
                 result:   &mut R,
                 &($( $T, )*):
                           &($( &$T, )*),
                 userdata: &mut Callback)
              where Callback: FnMut($( $T, )*) -> R + 'a
            {
                unsafe {
                    ptr::write(result, userdata($( $T.clone(), )*));
                }
            }
        }
    }
}

define_closure_types!(Cif0 Callback0 CallbackMut0 Closure0 ClosureMut0;
                      );
define_closure_types!(Cif1 Callback1 CallbackMut1 Closure1 ClosureMut1;
                      A);
define_closure_types!(Cif2 Callback2 CallbackMut2 Closure2 ClosureMut2;
                      A B);
define_closure_types!(Cif3 Callback3 CallbackMut3 Closure3 ClosureMut3;
                      A B C);
define_closure_types!(Cif4 Callback4 CallbackMut4 Closure4 ClosureMut4;
                      A B C D);
define_closure_types!(Cif5 Callback5 CallbackMut5 Closure5 ClosureMut5;
                      A B C D E);
define_closure_types!(Cif6 Callback6 CallbackMut6 Closure6 ClosureMut6;
                      A B C D E F);
define_closure_types!(Cif7 Callback7 CallbackMut7 Closure7 ClosureMut7;
                      A B C D E F G);
define_closure_types!(Cif8 Callback8 CallbackMut8 Closure8 ClosureMut8;
                      A B C D E F G H);
define_closure_types!(Cif9 Callback9 CallbackMut9 Closure9 ClosureMut9;
                      A B C D E F G H I);
define_closure_types!(Cif10 Callback10 CallbackMut10 Closure10 ClosureMut10;
                      A B C D E F G H I J);
define_closure_types!(Cif11 Callback11 CallbackMut11 Closure11 ClosureMut11;
                      A B C D E F G H I J K);
define_closure_types!(Cif12 Callback12 CallbackMut12 Closure12 ClosureMut12;
                      A B C D E F G H I J K L);

#[cfg(test)]
mod test {
    use super::*;
    use super::types::*;

    #[test]
    fn new_with_cif() {
        let x: u64 = 1;
        let f = |y: u64, z: u64| x + y + z;

        let type_   = u64::get_type();
        let cif     = Cif2::new(type_.clone(), type_.clone(), type_.clone());
        let closure = Closure2::new_with_cif(cif, &f);

        assert_eq!(12, closure.code_ptr()(5, 6));
    }

    #[test]
    fn new_with_cif_mut() {
        let mut x: u64 = 0;
        let mut f = |y: u64| { x += y; x };

        let type_   = u64::get_type();
        let cif     = Cif1::new(type_.clone(), type_.clone());
        let closure = ClosureMut1::new_with_cif(cif, &mut f);

        let counter = closure.code_ptr();

        assert_eq!(5, counter(5));
        assert_eq!(6, counter(1));
        assert_eq!(8, counter(2));
    }

    #[test]
    fn new() {
        let x: u64 = 1;
        let f = |y: u64, z: u64| x + y + z;

        let closure = Closure2::new(&f);

        assert_eq!(12, closure.code_ptr()(5, 6));
    }

    #[test]
    fn new_mut() {
        let mut x: u64 = 0;
        let mut f = |y: u32| { x += y as u64; x };

        let closure = ClosureMut1::new(&mut f);
        let counter = closure.code_ptr();

        assert_eq!(5, counter(5));
        assert_eq!(6, counter(1));
        assert_eq!(8, counter(2));
    }
}