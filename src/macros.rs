/// Run some code that returns a number and return Ok(result) if >= 0 or Err(result) if < 0
macro_rules! code {
    ( $try:expr ) => {{
        let result = $try;
        if result < 0 {
            Err(result)
        } else {
            Ok(())
        }
    }}
}

/// Initialize a pointer with the given callback
macro_rules! ptr_init {
    ( $typ:ty, $initfn:expr ) => {{
        let mut p = ptr::null_mut() as $typ;
        let ret = $initfn(&mut p);

        if ret < 0 {
            Err(ret)
        } else {
            Ok(p)
        }
    }}
}
