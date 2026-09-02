/* Criterion FFI over vendored dr_wav. Not on the ryf product path. */
#define DR_WAV_IMPLEMENTATION
#define DR_WAV_NO_STDIO
#include "dr_wav.h"

#include <stdint.h>
#include <string.h>

int ryf_drwav_decode_f32(
    const void *data,
    size_t data_size,
    float **out,
    uint64_t *frames,
    unsigned *channels)
{
    unsigned ch = 0;
    unsigned rate = 0;
    drwav_uint64 n = 0;
    float *pcm;

    if (out == NULL || frames == NULL || channels == NULL) {
        return 0;
    }
    *out = NULL;
    *frames = 0;
    *channels = 0;

    pcm = drwav_open_memory_and_read_pcm_frames_f32(data, data_size, &ch, &rate, &n, NULL);
    if (pcm == NULL) {
        return 0;
    }
    *out = pcm;
    *frames = n;
    *channels = ch;
    return 1;
}

void ryf_drwav_free(void *p)
{
    drwav_free(p, NULL);
}

static int encode(
    drwav_uint32 format,
    drwav_uint32 bits,
    const void *pcm,
    uint64_t frames,
    unsigned channels,
    unsigned sample_rate,
    void **out,
    size_t *out_size)
{
    drwav wav;
    drwav_data_format fmt;
    void *data = NULL;
    size_t size = 0;
    drwav_uint64 written;

    if (out == NULL || out_size == NULL || pcm == NULL) {
        return 0;
    }
    *out = NULL;
    *out_size = 0;

    memset(&fmt, 0, sizeof(fmt));
    fmt.container = drwav_container_riff;
    fmt.format = format;
    fmt.channels = channels;
    fmt.sampleRate = sample_rate;
    fmt.bitsPerSample = bits;

    if (!drwav_init_memory_write_sequential_pcm_frames(&wav, &data, &size, &fmt, frames, NULL)) {
        return 0;
    }
    written = drwav_write_pcm_frames(&wav, frames, pcm);
    drwav_uninit(&wav);
    if (written != frames || data == NULL) {
        drwav_free(data, NULL);
        return 0;
    }
    *out = data;
    *out_size = size;
    return 1;
}

int ryf_drwav_encode_s16(
    const int16_t *pcm,
    uint64_t frames,
    unsigned channels,
    unsigned sample_rate,
    void **out,
    size_t *out_size)
{
    return encode(DR_WAVE_FORMAT_PCM, 16, pcm, frames, channels, sample_rate, out, out_size);
}

int ryf_drwav_encode_f32(
    const float *pcm,
    uint64_t frames,
    unsigned channels,
    unsigned sample_rate,
    void **out,
    size_t *out_size)
{
    return encode(DR_WAVE_FORMAT_IEEE_FLOAT, 32, pcm, frames, channels, sample_rate, out, out_size);
}
