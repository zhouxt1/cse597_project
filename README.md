This is a simple data flow analysis scheme for catching aliasing bugs in Rust. 

To run it, simply use 
`run.sh tests/test1.rs`

Some other test files are included in the `tests/` folder. 

You can see the log tells: `ERROR: This write is to a REVOKED pointer: p1`. This indicates a potential Stacked Borrow violation. 