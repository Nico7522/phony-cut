//! Phone sound: AAC (from the scrcpy audio socket) → interleaved `f32`
//! samples at the PC's output rate, with FFmpeg (LGPL build: native AAC
//! decoder and swresample).

use std::ptr;

use ffmpeg::codec;
use ffmpeg::format::sample::Type as SampleType;
use ffmpeg::format::Sample;
use ffmpeg::software::resampling;
use ffmpeg::util::frame;
use ffmpeg::ChannelLayout;
use ffmpeg_next as ffmpeg;

use crate::decode::init_ffmpeg;

/// What scrcpy captures (`AudioConfig`): 48 kHz stereo.
pub const DEVICE_RATE: u32 = 48_000;
pub const DEVICE_CHANNELS: u16 = 2;

/// Zeroed padding FFmpeg requires after extradata (`AV_INPUT_BUFFER_PADDING_SIZE`).
const FFMPEG_PADDING: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum AudioDecodeError {
    #[error("FFmpeg has no AAC decoder")]
    NoDecoder,
    #[error("out of memory")]
    OutOfMemory,
    #[error("FFmpeg error: {0}")]
    Ffmpeg(#[from] ffmpeg::Error),
}

/// Decodes raw AAC frames (no ADTS) and converts them to interleaved `f32`.
pub struct AacDecoder {
    decoder: ffmpeg::decoder::Audio,
    decoded: frame::Audio,
    resampler: Option<resampling::Context>,
    out_rate: u32,
    out_layout: ChannelLayout,
    out_channels: usize,
}

impl AacDecoder {
    /// `config`: the AudioSpecificConfig sent as the first audio packet
    /// (`11 90` = AAC-LC, 48 kHz, stereo). The output has `out_channels`
    /// channels at `out_rate` Hz (the PC's sound device).
    pub fn new(config: &[u8], out_rate: u32, out_channels: u16) -> Result<Self, AudioDecodeError> {
        init_ffmpeg();
        let codec = ffmpeg::decoder::find(codec::Id::AAC).ok_or(AudioDecodeError::NoDecoder)?;
        let mut context = codec::Context::new_with_codec(codec);
        // SAFETY: `context` was just allocated by avcodec_alloc_context3 and
        // is not opened yet; FFmpeg requires extradata to be av_malloc'ed with
        // zeroed padding and frees it with the context.
        unsafe {
            let raw = context.as_mut_ptr();
            let extradata = ffmpeg::ffi::av_mallocz(config.len() + FFMPEG_PADDING).cast::<u8>();
            if extradata.is_null() {
                return Err(AudioDecodeError::OutOfMemory);
            }
            ptr::copy_nonoverlapping(config.as_ptr(), extradata, config.len());
            (*raw).extradata = extradata;
            (*raw).extradata_size = i32::try_from(config.len()).unwrap_or(0);
            (*raw).sample_rate = DEVICE_RATE as i32;
        }
        let decoder = context.decoder().audio()?;
        let out_channels = usize::from(out_channels.max(1));
        Ok(Self {
            decoder,
            decoded: frame::Audio::empty(),
            resampler: None,
            out_rate,
            out_layout: ChannelLayout::default(out_channels as i32),
            out_channels,
        })
    }

    /// Decoder for the AAC track of a file (its extradata is the config).
    pub fn from_parameters(
        params: &codec::Parameters,
        out_rate: u32,
        out_channels: u16,
    ) -> Result<Self, AudioDecodeError> {
        // SAFETY: plain reads of the stream's codec parameters, valid while
        // `params` lives; the bytes are copied before returning.
        let config = unsafe {
            let raw = params.as_ptr();
            let size = usize::try_from((*raw).extradata_size).unwrap_or(0);
            if (*raw).extradata.is_null() || size == 0 {
                Vec::new()
            } else {
                std::slice::from_raw_parts((*raw).extradata, size).to_vec()
            }
        };
        Self::new(&config, out_rate, out_channels)
    }

    /// Decodes one AAC frame and appends its samples (interleaved) to `out`.
    pub fn decode(&mut self, data: &[u8], out: &mut Vec<f32>) -> Result<(), AudioDecodeError> {
        let packet = ffmpeg::Packet::copy(data);
        self.decoder.send_packet(&packet)?;
        loop {
            match self.decoder.receive_frame(&mut self.decoded) {
                Ok(()) => self.convert(out)?,
                Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::util::error::EAGAIN => {
                    return Ok(())
                }
                Err(ffmpeg::Error::Eof) => return Ok(()),
                Err(err) => return Err(err.into()),
            }
        }
    }

    fn convert(&mut self, out: &mut Vec<f32>) -> Result<(), AudioDecodeError> {
        let input = &self.decoded;
        let resampler = match &mut self.resampler {
            Some(resampler) => resampler,
            slot => slot.insert(resampling::Context::get(
                input.format(),
                input.channel_layout(),
                input.rate(),
                Sample::F32(SampleType::Packed),
                self.out_layout,
                self.out_rate,
            )?),
        };
        // Room for the resampled frame plus what swresample kept from the
        // previous one, so nothing piles up inside it.
        let capacity =
            input.samples() * self.out_rate as usize / input.rate().max(1) as usize + 256;
        let mut converted =
            frame::Audio::new(Sample::F32(SampleType::Packed), capacity, self.out_layout);
        resampler.run(input, &mut converted)?;

        let values = converted.samples() * self.out_channels;
        let bytes = &converted.data(0)[..values * 4];
        out.extend(
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_ne_bytes(*b)),
        );
        Ok(())
    }
}
