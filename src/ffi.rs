use std::os::raw::{c_char, c_uchar};
use std::ffi::{CStr, CString};
use crate::{UniversalSchematic, formats::{litematic, schematic}};

#[repr(C)]
pub struct ByteArray {
    data: *mut c_uchar,
    len: usize,
}


#[no_mangle]
pub extern "C" fn convert_schematic(
    input_data: *const c_char,
    input_len: usize,
    output_format: *const c_char,
) -> ByteArray {
    let input_slice = unsafe {
        std::slice::from_raw_parts(input_data as *const u8, input_len)
    };

    let format = unsafe {
        CStr::from_ptr(output_format)
            .to_str()
            .unwrap_or("litematic")
    };

    let result = match format {
        "litematic" => {
            if schematic::is_schematic(input_slice) {
                let schematic = schematic::from_schematic(input_slice).unwrap();
                litematic::to_litematic(&schematic).unwrap()
            } else {
                Vec::new()
            }
        },
        "schem" => {
            if litematic::is_litematic(input_slice) {
                let schematic = litematic::from_litematic(input_slice).unwrap();
                schematic::to_schematic(&schematic).unwrap()
            } else {
                Vec::new()
            }
        },
        _ => Vec::new()
    };

    let mut boxed_slice = result.into_boxed_slice();
    let len = boxed_slice.len();
    let data = Box::into_raw(boxed_slice) as *mut c_uchar;

    ByteArray { data, len }
}

#[no_mangle]
pub extern "C" fn free_byte_array(array: ByteArray) {
    unsafe {
        let _ = Box::from_raw(std::slice::from_raw_parts_mut(array.data, array.len));
    }
}