fn main() {
    let z = false;
    let mut x = 1;
    let mut y = 2;
    let r = &mut y as *mut i32;
    let p = if z { &mut x } else { 
    unsafe { &mut *r }}; 
    if !z { unsafe { *r += 1; } }; 
    *p = 10;
}