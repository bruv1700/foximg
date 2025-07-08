//! This module provides additions for foximg.

use raylib::prelude::*;

use std::{fmt::Display, num::NonZeroU32};

use crate::Rgba;

/// Number of repetitions in an animated image.
#[derive(Copy, Clone)]
pub enum AnimationLoops {
    /// Finite number of repetitions
    Finite(NonZeroU32),
    /// Infinite number of repetitions
    Infinite,
}

impl Display for AnimationLoops {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnimationLoops::Finite(i) => write!(f, "{i}"),
            AnimationLoops::Infinite => write!(f, "infinite"),
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for AnimationLoops {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            AnimationLoops::Finite(i) => serializer.serialize_some(i),
            AnimationLoops::Infinite => serializer.serialize_f32(f32::INFINITY),
        }
    }
}

/// Trait for animated image decoders that can get how many times the animation iterates.
pub trait AnimationLoopsDecoder {
    /// Returns how many times the decoded animation iterates.
    fn get_loop_count(&self) -> AnimationLoops;
}

impl From<Color> for Rgba<u8> {
    #[inline]
    fn from(c: Color) -> Self {
        Self([c.a, c.b, c.g, c.r])
    }
}
