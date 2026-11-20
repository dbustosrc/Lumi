use super::{PcmStreamFormat, pcm_frame_limits, queue_frames};
use std::collections::VecDeque;

#[test]
fn pcm_limits_follow_advertised_input_duration_across_sample_rates() {
    assert_eq!(
        pcm_frame_limits(
            PcmStreamFormat {
                sample_rate: 48_000,
                channels: 2,
                bit_depth: 16,
            },
            48_000,
            384_000,
        ),
        Some((4, 96_000, 96_000))
    );
    assert_eq!(
        pcm_frame_limits(
            PcmStreamFormat {
                sample_rate: 48_000,
                channels: 2,
                bit_depth: 16,
            },
            44_100,
            384_000,
        ),
        Some((4, 96_000, 88_200))
    );
    assert_eq!(
        pcm_frame_limits(
            PcmStreamFormat {
                sample_rate: 48_000,
                channels: 1,
                bit_depth: 24,
            },
            48_000,
            24,
        ),
        Some((3, 8, 8))
    );
}

#[test]
fn pcm_limits_reject_unsupported_or_zero_formats() {
    for format in [
        PcmStreamFormat {
            sample_rate: 48_000,
            channels: 2,
            bit_depth: 8,
        },
        PcmStreamFormat {
            sample_rate: 0,
            channels: 2,
            bit_depth: 16,
        },
        PcmStreamFormat {
            sample_rate: 48_000,
            channels: 0,
            bit_depth: 16,
        },
    ] {
        assert_eq!(pcm_frame_limits(format, 48_000, 384_000), None);
    }
    assert_eq!(
        pcm_frame_limits(
            PcmStreamFormat {
                sample_rate: 48_000,
                channels: 2,
                bit_depth: 16,
            },
            0,
            384_000,
        ),
        None
    );
}

#[test]
fn pcm_queue_discards_old_frames_and_truncates_oversized_packets() {
    let mut queue = VecDeque::from([(1., 1.), (2., 2.), (3., 3.)]);
    queue_frames(&mut queue, &[(4., 4.), (5., 5.), (6., 6.)], 4);
    assert_eq!(queue, VecDeque::from([(3., 3.), (4., 4.), (5., 5.), (6., 6.)]));

    queue_frames(
        &mut queue,
        &[(7., 7.), (8., 8.), (9., 9.), (10., 10.)],
        3,
    );
    assert_eq!(queue, VecDeque::from([(8., 8.), (9., 9.), (10., 10.)]));

    queue_frames(&mut queue, &[(11., 11.)], 0);
    assert!(queue.is_empty());
}
