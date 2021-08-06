use alsa_sys as ffi;
use nix::errno::Errno;
use std::collections::VecDeque;
use std::ffi::CStr;
use std::ptr;

#[derive(Debug)]
pub enum Error {
    InitError(Errno),
}

impl From<i32> for Error {
    fn from(val: i32) -> Error {
        let errno = Errno::from_i32(-val);
        Error::InitError(errno)
    }
}

pub struct DeviceConfig {
    /// The target amount of time to store buffered audio for. The sound driver will use something
    /// close to this number, but it might not be exact.
    pub buffer_target_us: u32,
    /// The number of channels for playback. Channel data is always interleaved.
    pub channels: u32,
    /// The target amount of time to process before asking the application for more data. The sound
    /// driver will use something close to this number, but it might not be exact.
    pub period_target_us: u32,
    /// The constant sample rate in hz to output audio at
    pub sample_rate: u32,
}

#[derive(Debug)]
pub struct Device {
    buffer_size: u64,
    handle: *mut ffi::snd_pcm_t,
    period_size: u64,
    sample_rate: u32,
    user_buffer: VecDeque<f32>,
}

impl Device {
    pub fn with_config(config: &DeviceConfig) -> Result<Self, Error> {
        let handle = unsafe {
            let device_name = CStr::from_bytes_with_nul_unchecked(b"default\0").as_ptr();
            ptr_init!(*mut ffi::snd_pcm_t, |p| ffi::snd_pcm_open(
                    p,
                    device_name,
                    ffi::SND_PCM_STREAM_PLAYBACK,
                    ffi::SND_PCM_NONBLOCK
            ))?
        };

        let fmt = if cfg!(target_endian = "big") {
            ffi::SND_PCM_FORMAT_FLOAT_BE
        } else {
            ffi::SND_PCM_FORMAT_FLOAT_LE
        };

        let mut hw_params = HwParams::new()?;
        let mut sample_rate = config.sample_rate;
        let mut buffer_time = config.buffer_target_us;
        let mut period_time = config.period_target_us;

        unsafe {
            code!(ffi::snd_pcm_hw_params_any(handle, hw_params.as_mut_ptr()))?;
            code!(ffi::snd_pcm_hw_params_set_channels(handle, hw_params.as_mut_ptr(), config.channels))?;
            code!(ffi::snd_pcm_hw_params_set_rate_near(handle, hw_params.as_mut_ptr(), &mut sample_rate, &mut 0))?;
            code!(ffi::snd_pcm_hw_params_set_rate_resample(handle, hw_params.as_mut_ptr(), 1))?;
            code!(ffi::snd_pcm_hw_params_set_format(handle, hw_params.as_mut_ptr(), fmt))?;
            code!(ffi::snd_pcm_hw_params_set_access(handle, hw_params.as_mut_ptr(), ffi::SND_PCM_ACCESS_RW_INTERLEAVED))?;
            code!(ffi::snd_pcm_hw_params_set_buffer_time_near(handle, hw_params.as_mut_ptr(), &mut buffer_time, &mut 0))?;
            code!(ffi::snd_pcm_hw_params_set_period_time_near(handle, hw_params.as_mut_ptr(), &mut period_time, &mut 0))?;
            code!(ffi::snd_pcm_hw_params(handle, hw_params.as_mut_ptr()))?;
        }

        let buffer_size = hw_params.buffer_size()?;
        let period_size = hw_params.period_size()?;
        let start_threshold = (buffer_size / period_size) * period_size;
        let can_transfer_threshold = period_size;

        unsafe {
            let mut sw_params = SwParams::new()?;

            code!(ffi::snd_pcm_sw_params_current(handle, sw_params.as_mut_ptr()))?;
            code!(ffi::snd_pcm_sw_params_set_start_threshold(handle, sw_params.as_mut_ptr(), start_threshold))?;
            code!(ffi::snd_pcm_sw_params_set_avail_min(handle, sw_params.as_mut_ptr(), can_transfer_threshold))?;
            code!(ffi::snd_pcm_sw_params(handle, sw_params.as_mut_ptr()))?;

            code!(ffi::snd_pcm_prepare(handle))?;
        }

        let user_buffer = VecDeque::with_capacity(buffer_size as usize);

        Ok(Self { buffer_size, handle, period_size, sample_rate, user_buffer })
    }

    pub fn run<F>(mut self, mut data_callback: F)
    where F: FnMut(&mut VecDeque<f32>, usize) {

        // Fill the buffer first
        let wanted = self.user_buffer.capacity();
        data_callback(&mut self.user_buffer, wanted);

        loop {
            unsafe {
                let (buf, _) = self.user_buffer.as_slices();

                let ret = ffi::snd_pcm_writei(self.handle, buf.as_ptr() as _, buf.len() as u64);
                let errno = Errno::from_i32(-ret as i32);

                if Errno::EAGAIN == errno {
                    let ret = ffi::snd_pcm_wait(self.handle, -1);
                    if ret < 0 { panic!("Failed to poll device") }
                    continue;
                }

                if ret < 0 {
                    panic!("Error writing to sound device");
                }

                for _ in 0..ret {
                    self.user_buffer.pop_front();
                }

                data_callback(&mut self.user_buffer, ret as usize);
            }
        }
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            ffi::snd_pcm_drop(self.handle);
            ffi::snd_pcm_hw_free(self.handle);
        }
    }
}

struct HwParams(*mut ffi::snd_pcm_hw_params_t);

impl HwParams {
    fn new() -> Result<Self, Error> {
        let pointer = ptr_init!(
            *mut ffi::snd_pcm_hw_params_t,
            |p| unsafe { ffi::snd_pcm_hw_params_malloc(p) }
        )?;

        Ok(Self(pointer))
    }

    fn as_mut_ptr(&mut self) -> *mut ffi::snd_pcm_hw_params_t {
        self.0
    }

    fn buffer_size(&self) -> Result<u64, Error> {
        let mut buffer_size = 0;
        unsafe {
            code!(ffi::snd_pcm_hw_params_get_buffer_size(self.0, &mut buffer_size))?;
        }

        Ok(buffer_size)
    }

    fn period_size(&self) -> Result<u64, Error> {
        let mut period_size = 0;
        unsafe {
            code!(ffi::snd_pcm_hw_params_get_period_size(self.0, &mut period_size, &mut 0))?;
        }

        Ok(period_size)
    }
}

impl Drop for HwParams {
    fn drop(&mut self) {
        unsafe { ffi::snd_pcm_hw_params_free(self.0) }
    }
}

struct SwParams(*mut ffi::snd_pcm_sw_params_t);

impl SwParams {
    fn new() -> Result<Self, Error> {
        let pointer = ptr_init!(
            *mut ffi::snd_pcm_sw_params_t,
            |p| unsafe { ffi::snd_pcm_sw_params_malloc(p) }
        )?;

        Ok(Self(pointer))
    }

    fn as_mut_ptr(&mut self) -> *mut ffi::snd_pcm_sw_params_t {
        self.0
    }
}

impl Drop for SwParams {
    fn drop(&mut self) {
        unsafe { ffi::snd_pcm_sw_params_free(self.0) }
    }
}
