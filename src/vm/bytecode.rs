//! Byte-level helpers and predicates for the interpreter loop.

use crate::{JayError, JayResult};

pub(super) fn read_u1(bytes: &[u8], pc: &mut usize) -> JayResult<u8> {
    if *pc >= bytes.len() {
        return Err(JayError::new("unexpected end of bytecode"));
    }
    let value = bytes[*pc];
    *pc += 1;
    Ok(value)
}

pub(super) fn read_u2(bytes: &[u8], pc: &mut usize) -> JayResult<u16> {
    let high = read_u1(bytes, pc)? as u16;
    let low = read_u1(bytes, pc)? as u16;
    Ok((high << 8) | low)
}

pub(super) fn read_i2(bytes: &[u8], pc: &mut usize) -> JayResult<i16> {
    Ok(read_u2(bytes, pc)? as i16)
}

pub(super) fn branch_target(code_len: usize, opcode_pc: usize, offset: i16) -> JayResult<usize> {
    let target = opcode_pc as i64 + offset as i64;
    if target < 0 || target >= code_len as i64 {
        return Err(JayError::new(format!(
            "branch target {target} out of bytecode range 0..{code_len}"
        )));
    }

    Ok(target as usize)
}

pub(super) fn int_branch_taken(opcode: u8, value: i32) -> JayResult<bool> {
    match opcode {
        0x99 => Ok(value == 0),
        0x9a => Ok(value != 0),
        0x9b => Ok(value < 0),
        0x9c => Ok(value >= 0),
        0x9d => Ok(value > 0),
        0x9e => Ok(value <= 0),
        _ => Err(JayError::new(format!(
            "unsupported integer branch opcode 0x{opcode:02x}"
        ))),
    }
}

pub(super) fn int_compare_branch_taken(opcode: u8, left: i32, right: i32) -> JayResult<bool> {
    match opcode {
        0x9f => Ok(left == right),
        0xa0 => Ok(left != right),
        0xa1 => Ok(left < right),
        0xa2 => Ok(left >= right),
        0xa3 => Ok(left > right),
        0xa4 => Ok(left <= right),
        _ => Err(JayError::new(format!(
            "unsupported integer comparison branch opcode 0x{opcode:02x}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_target_uses_opcode_pc_and_signed_offsets() {
        assert_eq!(branch_target(20, 5, 14).unwrap(), 19);
        assert_eq!(branch_target(20, 15, -10).unwrap(), 5);
    }

    #[test]
    fn branch_target_rejects_out_of_range_targets() {
        let before_start = branch_target(10, 0, -1).unwrap_err();
        assert!(
            before_start
                .to_string()
                .contains("branch target -1 out of bytecode range")
        );

        let at_end = branch_target(10, 8, 2).unwrap_err();
        assert!(
            at_end
                .to_string()
                .contains("branch target 10 out of bytecode range")
        );
    }

    #[test]
    fn integer_zero_branch_predicates_match_jvm_conditions() {
        assert!(int_branch_taken(0x99, 0).unwrap());
        assert!(!int_branch_taken(0x99, 1).unwrap());

        assert!(int_branch_taken(0x9a, 1).unwrap());
        assert!(!int_branch_taken(0x9a, 0).unwrap());

        assert!(int_branch_taken(0x9b, -1).unwrap());
        assert!(!int_branch_taken(0x9b, 0).unwrap());

        assert!(int_branch_taken(0x9c, 0).unwrap());
        assert!(!int_branch_taken(0x9c, -1).unwrap());

        assert!(int_branch_taken(0x9d, 1).unwrap());
        assert!(!int_branch_taken(0x9d, 0).unwrap());

        assert!(int_branch_taken(0x9e, 0).unwrap());
        assert!(!int_branch_taken(0x9e, 1).unwrap());
    }

    #[test]
    fn integer_comparison_branch_predicates_match_jvm_conditions() {
        assert!(int_compare_branch_taken(0x9f, 2, 2).unwrap());
        assert!(!int_compare_branch_taken(0x9f, 2, 3).unwrap());

        assert!(int_compare_branch_taken(0xa0, 2, 3).unwrap());
        assert!(!int_compare_branch_taken(0xa0, 2, 2).unwrap());

        assert!(int_compare_branch_taken(0xa1, 2, 3).unwrap());
        assert!(!int_compare_branch_taken(0xa1, 3, 2).unwrap());

        assert!(int_compare_branch_taken(0xa2, 3, 2).unwrap());
        assert!(int_compare_branch_taken(0xa2, 2, 2).unwrap());
        assert!(!int_compare_branch_taken(0xa2, 2, 3).unwrap());

        assert!(int_compare_branch_taken(0xa3, 3, 2).unwrap());
        assert!(!int_compare_branch_taken(0xa3, 2, 2).unwrap());

        assert!(int_compare_branch_taken(0xa4, 2, 3).unwrap());
        assert!(int_compare_branch_taken(0xa4, 2, 2).unwrap());
        assert!(!int_compare_branch_taken(0xa4, 3, 2).unwrap());
    }
}
