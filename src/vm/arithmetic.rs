//! Pure integer and long arithmetic for the JVM opcodes the interpreter supports.
//!
//! Every function here is free of VM state so the wrapping, masking, and
//! division-by-zero rules can be unit tested directly. The opcode dispatch in
//! `interpreter.rs` pops operands and delegates to these helpers.

use crate::{JayError, JayResult};

/// Applies a two-operand `int` opcode (`iadd` through `ixor`).
pub(super) fn int_binary(opcode: u8, left: i32, right: i32) -> JayResult<i32> {
    match opcode {
        0x60 => Ok(left.wrapping_add(right)),
        0x64 => Ok(left.wrapping_sub(right)),
        0x68 => Ok(left.wrapping_mul(right)),
        0x6c => checked_int_division(left, right, i32::wrapping_div),
        0x70 => checked_int_division(left, right, i32::wrapping_rem),
        0x78 => Ok(left.wrapping_shl(int_shift_count(right))),
        0x7a => Ok(left.wrapping_shr(int_shift_count(right))),
        0x7c => Ok(((left as u32) >> int_shift_count(right)) as i32),
        0x7e => Ok(left & right),
        0x80 => Ok(left | right),
        0x82 => Ok(left ^ right),
        _ => Err(JayError::new(format!(
            "unsupported int arithmetic opcode 0x{opcode:02x}"
        ))),
    }
}

/// Applies a two-operand `long` opcode (`ladd` through `lxor`, excluding shifts).
pub(super) fn long_binary(opcode: u8, left: i64, right: i64) -> JayResult<i64> {
    match opcode {
        0x61 => Ok(left.wrapping_add(right)),
        0x65 => Ok(left.wrapping_sub(right)),
        0x69 => Ok(left.wrapping_mul(right)),
        0x6d => checked_long_division(left, right, i64::wrapping_div),
        0x71 => checked_long_division(left, right, i64::wrapping_rem),
        0x7f => Ok(left & right),
        0x81 => Ok(left | right),
        0x83 => Ok(left ^ right),
        _ => Err(JayError::new(format!(
            "unsupported long arithmetic opcode 0x{opcode:02x}"
        ))),
    }
}

/// Applies a `long` shift opcode (`lshl`, `lshr`, `lushr`); the count is an `int`.
pub(super) fn long_shift(opcode: u8, value: i64, count: i32) -> JayResult<i64> {
    let count = (count & 0x3f) as u32;
    match opcode {
        0x79 => Ok(value.wrapping_shl(count)),
        0x7b => Ok(value.wrapping_shr(count)),
        0x7d => Ok(((value as u64) >> count) as i64),
        _ => Err(JayError::new(format!(
            "unsupported long shift opcode 0x{opcode:02x}"
        ))),
    }
}

/// Implements `lcmp`: -1, 0, or 1.
pub(super) fn long_compare(left: i64, right: i64) -> i32 {
    match left.cmp(&right) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// Applies an `int`-to-`int` narrowing opcode (`i2b`, `i2c`, `i2s`).
pub(super) fn narrow_int(opcode: u8, value: i32) -> JayResult<i32> {
    match opcode {
        0x91 => Ok(value as i8 as i32),
        0x92 => Ok(value as u16 as i32),
        0x93 => Ok(value as i16 as i32),
        _ => Err(JayError::new(format!(
            "unsupported int narrowing opcode 0x{opcode:02x}"
        ))),
    }
}

/// Implements `fcmpl` (`nan_result` -1) and `fcmpg` (`nan_result` 1).
pub(super) fn float_compare(left: f32, right: f32, nan_result: i32) -> i32 {
    if left.is_nan() || right.is_nan() {
        nan_result
    } else if left > right {
        1
    } else if left == right {
        0
    } else {
        -1
    }
}

/// The `ArithmeticException` Java raises for integer division by zero.
fn division_by_zero() -> JayError {
    JayError::fault(
        "java/lang/ArithmeticException",
        Some("/ by zero".to_string()),
    )
}

fn int_shift_count(count: i32) -> u32 {
    (count & 0x1f) as u32
}

fn checked_int_division(left: i32, right: i32, op: fn(i32, i32) -> i32) -> JayResult<i32> {
    if right == 0 {
        return Err(division_by_zero());
    }
    Ok(op(left, right))
}

fn checked_long_division(left: i64, right: i64, op: fn(i64, i64) -> i64) -> JayResult<i64> {
    if right == 0 {
        return Err(division_by_zero());
    }
    Ok(op(left, right))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_arithmetic_wraps_on_overflow() {
        assert_eq!(int_binary(0x60, i32::MAX, 1).unwrap(), i32::MIN);
        assert_eq!(int_binary(0x64, i32::MIN, 1).unwrap(), i32::MAX);
        assert_eq!(int_binary(0x68, i32::MAX, 2).unwrap(), -2);
        assert_eq!(int_binary(0x6c, i32::MIN, -1).unwrap(), i32::MIN);
        assert_eq!(int_binary(0x70, i32::MIN, -1).unwrap(), 0);
    }

    #[test]
    fn int_remainder_keeps_dividend_sign() {
        assert_eq!(int_binary(0x70, 10, 3).unwrap(), 1);
        assert_eq!(int_binary(0x70, -17, 3).unwrap(), -2);
        assert_eq!(int_binary(0x70, 17, -3).unwrap(), 2);
    }

    #[test]
    fn int_division_by_zero_is_an_arithmetic_exception_fault() {
        for opcode in [0x6c, 0x70] {
            let error = int_binary(opcode, 1, 0).unwrap_err();
            assert_eq!(error.fault_class(), Some("java/lang/ArithmeticException"));
            assert_eq!(error.fault_message(), Some("/ by zero"));
        }
    }

    #[test]
    fn int_shifts_mask_the_count_to_five_bits() {
        assert_eq!(int_binary(0x78, 1, 33).unwrap(), 2);
        assert_eq!(int_binary(0x7a, -16, 34).unwrap(), -4);
        assert_eq!(int_binary(0x7c, -1, 28).unwrap(), 15);
    }

    #[test]
    fn int_bitwise_operators() {
        assert_eq!(int_binary(0x7e, 10, 3).unwrap(), 2);
        assert_eq!(int_binary(0x80, 10, 3).unwrap(), 11);
        assert_eq!(int_binary(0x82, 10, 3).unwrap(), 9);
    }

    #[test]
    fn long_arithmetic_wraps_and_divides() {
        assert_eq!(long_binary(0x61, i64::MAX, 1).unwrap(), i64::MIN);
        assert_eq!(long_binary(0x65, 5, 7).unwrap(), -2);
        assert_eq!(long_binary(0x69, 1 << 40, 3).unwrap(), 3298534883328);
        assert_eq!(long_binary(0x6d, i64::MIN, -1).unwrap(), i64::MIN);
        assert_eq!(long_binary(0x71, -17, 3).unwrap(), -2);
        assert!(long_binary(0x6d, 1, 0).is_err());
        assert!(long_binary(0x71, 1, 0).is_err());
    }

    #[test]
    fn long_bitwise_operators() {
        assert_eq!(long_binary(0x7f, 0b1100, 0b1010).unwrap(), 0b1000);
        assert_eq!(long_binary(0x81, 0b1100, 0b1010).unwrap(), 0b1110);
        assert_eq!(long_binary(0x83, 0b1100, 0b1010).unwrap(), 0b0110);
    }

    #[test]
    fn long_shifts_mask_the_count_to_six_bits() {
        assert_eq!(long_shift(0x79, 1, 65).unwrap(), 2);
        assert_eq!(long_shift(0x7b, -(1 << 40), 38).unwrap(), -4);
        assert_eq!(long_shift(0x7d, -(1 << 40), 60).unwrap(), 15);
    }

    #[test]
    fn long_compare_orders_values() {
        assert_eq!(long_compare(1, 2), -1);
        assert_eq!(long_compare(2, 2), 0);
        assert_eq!(long_compare(3, 2), 1);
        assert_eq!(long_compare(i64::MIN, i64::MAX), -1);
    }

    #[test]
    fn narrowing_truncates_and_sign_extends() {
        assert_eq!(narrow_int(0x91, 300).unwrap(), 44);
        assert_eq!(narrow_int(0x91, 200).unwrap(), -56);
        assert_eq!(narrow_int(0x92, -1).unwrap(), 65535);
        assert_eq!(narrow_int(0x93, 90000).unwrap(), 24464);
        assert_eq!(narrow_int(0x93, 40000).unwrap(), -25536);
    }

    #[test]
    fn float_compare_orders_values_and_routes_nan() {
        assert_eq!(float_compare(1.0, 2.0, 1), -1);
        assert_eq!(float_compare(2.0, 2.0, 1), 0);
        assert_eq!(float_compare(3.0, 2.0, 1), 1);
        assert_eq!(float_compare(f32::NAN, 2.0, 1), 1);
        assert_eq!(float_compare(f32::NAN, 2.0, -1), -1);
    }

    #[test]
    fn unknown_opcodes_are_rejected() {
        assert!(int_binary(0x00, 1, 1).is_err());
        assert!(long_binary(0x00, 1, 1).is_err());
        assert!(long_shift(0x00, 1, 1).is_err());
        assert!(narrow_int(0x00, 1).is_err());
    }
}
