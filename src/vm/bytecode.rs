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

pub(super) fn read_i4(bytes: &[u8], pc: &mut usize) -> JayResult<i32> {
    let high = read_u2(bytes, pc)? as u32;
    let low = read_u2(bytes, pc)? as u32;
    Ok(((high << 16) | low) as i32)
}

/// Advances `pc` past the zero padding that aligns `tableswitch`/`lookupswitch`
/// operands to a four-byte boundary relative to the start of the code array.
fn skip_switch_padding(bytes: &[u8], pc: &mut usize) -> JayResult<()> {
    while !(*pc).is_multiple_of(4) {
        if read_u1(bytes, pc)? != 0 {
            return Err(JayError::new("switch padding byte is nonzero"));
        }
    }
    Ok(())
}

/// Decodes the `tableswitch` operands after the opcode and returns the branch target for `key`.
pub(super) fn tableswitch_target(
    bytes: &[u8],
    opcode_pc: usize,
    pc: &mut usize,
    key: i32,
) -> JayResult<usize> {
    skip_switch_padding(bytes, pc)?;
    let default_offset = read_i4(bytes, pc)?;
    let low = read_i4(bytes, pc)?;
    let high = read_i4(bytes, pc)?;
    if low > high {
        return Err(JayError::new(format!(
            "tableswitch low {low} exceeds high {high}"
        )));
    }

    let entries = (high as i64 - low as i64 + 1) as usize;
    let table_start = *pc;
    *pc = table_start
        .checked_add(entries * 4)
        .ok_or_else(|| JayError::new("tableswitch table overflows bytecode"))?;
    if *pc > bytes.len() {
        return Err(JayError::new("unexpected end of bytecode in tableswitch"));
    }

    let offset = if key < low || key > high {
        default_offset
    } else {
        let mut entry_pc = table_start + (key as i64 - low as i64) as usize * 4;
        read_i4(bytes, &mut entry_pc)?
    };
    branch_target(bytes.len(), opcode_pc, offset)
}

/// Decodes the `lookupswitch` operands after the opcode and returns the branch target for `key`.
pub(super) fn lookupswitch_target(
    bytes: &[u8],
    opcode_pc: usize,
    pc: &mut usize,
    key: i32,
) -> JayResult<usize> {
    skip_switch_padding(bytes, pc)?;
    let default_offset = read_i4(bytes, pc)?;
    let pair_count = read_i4(bytes, pc)?;
    if pair_count < 0 {
        return Err(JayError::new(format!(
            "lookupswitch pair count {pair_count} is negative"
        )));
    }

    let mut offset = default_offset;
    for _ in 0..pair_count {
        let candidate = read_i4(bytes, pc)?;
        let candidate_offset = read_i4(bytes, pc)?;
        if candidate == key {
            offset = candidate_offset;
        }
    }
    branch_target(bytes.len(), opcode_pc, offset)
}

pub(super) fn branch_target(
    code_len: usize,
    opcode_pc: usize,
    offset: impl Into<i64>,
) -> JayResult<usize> {
    let target = opcode_pc as i64 + offset.into();
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

    fn i4(value: i32) -> [u8; 4] {
        value.to_be_bytes()
    }

    #[test]
    fn tableswitch_selects_indexed_offset_or_default() {
        // Opcode at pc 1 so three padding bytes follow.
        let mut bytes = vec![0x00, 0xaa, 0, 0];
        bytes.extend_from_slice(&i4(40)); // default
        bytes.extend_from_slice(&i4(2)); // low
        bytes.extend_from_slice(&i4(4)); // high
        bytes.extend_from_slice(&i4(10));
        bytes.extend_from_slice(&i4(20));
        bytes.extend_from_slice(&i4(30));
        bytes.resize(64, 0);

        for (key, expected) in [(2, 11), (3, 21), (4, 31), (1, 41), (5, 41)] {
            let mut pc = 2;
            assert_eq!(
                tableswitch_target(&bytes, 1, &mut pc, key).unwrap(),
                expected
            );
            assert_eq!(pc, 4 + 12 + 12);
        }
    }

    #[test]
    fn lookupswitch_matches_pairs_or_default() {
        let mut bytes = vec![0xab, 0, 0, 0];
        bytes.extend_from_slice(&i4(50)); // default
        bytes.extend_from_slice(&i4(2)); // npairs
        bytes.extend_from_slice(&i4(1));
        bytes.extend_from_slice(&i4(10));
        bytes.extend_from_slice(&i4(100));
        bytes.extend_from_slice(&i4(20));
        bytes.resize(64, 0);

        for (key, expected) in [(1, 10), (100, 20), (7, 50)] {
            let mut pc = 1;
            assert_eq!(
                lookupswitch_target(&bytes, 0, &mut pc, key).unwrap(),
                expected
            );
            assert_eq!(pc, 4 + 8 + 16);
        }
    }

    #[test]
    fn switch_padding_must_be_zero() {
        let bytes = vec![0xab, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut pc = 1;
        assert!(
            lookupswitch_target(&bytes, 0, &mut pc, 0)
                .unwrap_err()
                .to_string()
                .contains("padding")
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
