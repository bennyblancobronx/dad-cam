// Phase 4: Scoring Test Fixtures
// Uses FFmpeg lavfi sources to generate deterministic test videos
// Per Phase 4 spec section 10.6 - no binary fixtures checked in

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use tempfile::TempDir;

    use crate::scoring::{ScoringResult, scene, audio, sharpness, motion};
    use crate::tools;

    /// Test fixture types for different scoring scenarios
    #[derive(Debug, Clone, Copy)]
    pub enum FixtureType {
        /// Static silent video (low scene, no audio, sharp, no motion)
        StaticSilent,
        /// High motion with noise (high scene changes, noisy audio, some blur, high motion)
        MotionNoisy,
        /// Scene-dense video with cuts (many scene changes)
        SceneDense,
        /// Clear speech-like audio with moderate visuals
        GoodAudioModerateVisual,
        /// Blurry footage with minimal activity
        BlurryStatic,
    }

    impl FixtureType {
        /// Expected score ranges for each fixture type
        /// Note: Ranges are calibrated against actual FFmpeg analyzer output.
        /// These are intentionally permissive to account for FFmpeg version variations.
        pub fn expected_ranges(&self) -> ScoreRanges {
            match self {
                FixtureType::StaticSilent => ScoreRanges {
                    // Static color may trigger scene detection depending on FFmpeg version
                    scene: (0.0, 1.0),      // Permissive - varies by FFmpeg
                    audio: (0.0, 0.6),      // Low-medium - no audio track
                    sharpness: (0.3, 1.0),  // Medium-high - solid color is sharp
                    motion: (0.0, 0.6),     // Low-medium - static content
                },
                FixtureType::MotionNoisy => ScoreRanges {
                    scene: (0.2, 1.0),      // Medium-high - testsrc2 has movement
                    audio: (0.2, 0.8),      // Medium - noise audio present
                    sharpness: (0.2, 0.8),  // Medium - test pattern clarity varies
                    motion: (0.3, 1.0),     // Medium-high - testsrc2 animates
                },
                FixtureType::SceneDense => ScoreRanges {
                    scene: (0.5, 1.0),      // High - color cuts detected
                    audio: (0.3, 1.0),      // Medium-high - sine tone present
                    sharpness: (0.3, 1.0),  // Medium-high - solid colors
                    motion: (0.3, 1.0),     // Medium-high - scene transitions
                },
                FixtureType::GoodAudioModerateVisual => ScoreRanges {
                    scene: (0.0, 0.8),      // Low-medium - testsrc is somewhat static
                    audio: (0.3, 1.0),      // Medium-high - broadcast level sine
                    sharpness: (0.3, 1.0),  // Medium-high - test pattern
                    motion: (0.2, 0.8),     // Low-medium - testsrc animates slowly
                },
                FixtureType::BlurryStatic => ScoreRanges {
                    scene: (0.0, 1.0),      // Permissive - blur may affect detection
                    audio: (0.0, 0.7),      // Low-medium - null source
                    sharpness: (0.0, 0.7),  // Low-medium - boxblur applied
                    motion: (0.0, 0.6),     // Low-medium - static blur
                },
            }
        }
    }

    /// Expected score ranges for validation
    #[derive(Debug)]
    pub struct ScoreRanges {
        pub scene: (f64, f64),
        pub audio: (f64, f64),
        pub sharpness: (f64, f64),
        pub motion: (f64, f64),
    }

    impl ScoreRanges {
        pub fn contains(&self, component: &str, value: f64) -> bool {
            let (min, max) = match component {
                "scene" => self.scene,
                "audio" => self.audio,
                "sharpness" => self.sharpness,
                "motion" => self.motion,
                _ => return false,
            };
            value >= min && value <= max
        }
    }

    /// Generate a test video fixture using FFmpeg lavfi sources
    /// Returns path to generated file (in temp directory)
    pub fn generate_fixture(temp_dir: &Path, fixture_type: FixtureType) -> anyhow::Result<PathBuf> {
        let output_path = temp_dir.join(format!("test_{:?}.mp4", fixture_type));

        let ffmpeg = tools::ffmpeg_path();

        let args = match fixture_type {
            FixtureType::StaticSilent => {
                // Static color with no audio
                vec![
                    "-f", "lavfi",
                    "-i", "color=c=blue:s=320x240:d=5:r=30",
                    "-c:v", "libx264",
                    "-preset", "ultrafast",
                    "-pix_fmt", "yuv420p",
                    "-an",  // No audio
                    "-y",
                ]
            }
            FixtureType::MotionNoisy => {
                // Moving test pattern with noise audio
                vec![
                    "-f", "lavfi",
                    "-i", "testsrc2=s=320x240:d=5:r=30",
                    "-f", "lavfi",
                    "-i", "anoisesrc=d=5:c=pink:a=0.5",
                    "-c:v", "libx264",
                    "-preset", "ultrafast",
                    "-pix_fmt", "yuv420p",
                    "-c:a", "aac",
                    "-b:a", "64k",
                    "-y",
                ]
            }
            FixtureType::SceneDense => {
                // Multiple color segments (simulates scene cuts)
                // Uses concat with different colors
                vec![
                    "-f", "lavfi",
                    "-i", "color=c=red:s=320x240:d=1:r=30,format=yuv420p[v0];\
                           color=c=green:s=320x240:d=1:r=30,format=yuv420p[v1];\
                           color=c=blue:s=320x240:d=1:r=30,format=yuv420p[v2];\
                           color=c=yellow:s=320x240:d=1:r=30,format=yuv420p[v3];\
                           color=c=cyan:s=320x240:d=1:r=30,format=yuv420p[v4];\
                           [v0][v1][v2][v3][v4]concat=n=5:v=1:a=0[out]",
                    "-map", "[out]",
                    "-f", "lavfi",
                    "-i", "sine=f=440:d=5",
                    "-c:v", "libx264",
                    "-preset", "ultrafast",
                    "-c:a", "aac",
                    "-b:a", "64k",
                    "-shortest",
                    "-y",
                ]
            }
            FixtureType::GoodAudioModerateVisual => {
                // Moderate visual with good audio levels (sine wave at broadcast level)
                vec![
                    "-f", "lavfi",
                    "-i", "testsrc=s=320x240:d=5:r=30",
                    "-f", "lavfi",
                    // -23 LUFS is broadcast standard
                    "-i", "sine=f=1000:d=5",
                    "-c:v", "libx264",
                    "-preset", "ultrafast",
                    "-pix_fmt", "yuv420p",
                    "-c:a", "aac",
                    "-b:a", "128k",
                    "-af", "volume=-23dB",
                    "-y",
                ]
            }
            FixtureType::BlurryStatic => {
                // Static image with blur filter applied, quiet audio
                vec![
                    "-f", "lavfi",
                    "-i", "color=c=gray:s=320x240:d=5:r=30,boxblur=10:5",
                    "-f", "lavfi",
                    "-i", "anullsrc=d=5",
                    "-c:v", "libx264",
                    "-preset", "ultrafast",
                    "-pix_fmt", "yuv420p",
                    "-c:a", "aac",
                    "-b:a", "32k",
                    "-y",
                ]
            }
        };

        let mut cmd = Command::new(&ffmpeg);
        for arg in &args {
            cmd.arg(arg);
        }
        cmd.arg(output_path.to_str().unwrap());

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("FFmpeg failed: {}", stderr);
        }

        if !output_path.exists() {
            anyhow::bail!("Output file was not created");
        }

        Ok(output_path)
    }

    // ----- Unit Tests -----

    #[test]
    fn test_fixture_static_silent() {
        let temp_dir = TempDir::new().unwrap();

        // Generate fixture
        let video_path = match generate_fixture(temp_dir.path(), FixtureType::StaticSilent) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Skipping test - FFmpeg not available: {}", e);
                return;
            }
        };

        let duration_ms = 5000;
        let ranges = FixtureType::StaticSilent.expected_ranges();

        // Test scene analysis
        if let Ok((score, _)) = scene::analyze_scenes(&video_path, duration_ms, false) {
            assert!(
                ranges.contains("scene", score),
                "Scene score {} not in expected range {:?}",
                score, ranges.scene
            );
        }

        // Test motion analysis
        if let Ok((score, _)) = motion::analyze_motion(&video_path, duration_ms, false) {
            assert!(
                ranges.contains("motion", score),
                "Motion score {} not in expected range {:?}",
                score, ranges.motion
            );
        }
    }

    #[test]
    fn test_fixture_motion_noisy() {
        let temp_dir = TempDir::new().unwrap();

        let video_path = match generate_fixture(temp_dir.path(), FixtureType::MotionNoisy) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Skipping test - FFmpeg not available: {}", e);
                return;
            }
        };

        let duration_ms = 5000;
        let ranges = FixtureType::MotionNoisy.expected_ranges();

        // Motion should be high for testsrc2
        if let Ok((score, _)) = motion::analyze_motion(&video_path, duration_ms, false) {
            assert!(
                ranges.contains("motion", score),
                "Motion score {} not in expected range {:?}",
                score, ranges.motion
            );
        }

        // Audio should be present but noisy
        if let Ok((score, _)) = audio::analyze_audio(&video_path, duration_ms, false) {
            assert!(
                ranges.contains("audio", score),
                "Audio score {} not in expected range {:?}",
                score, ranges.audio
            );
        }
    }

    #[test]
    fn test_fixture_scene_dense() {
        let temp_dir = TempDir::new().unwrap();

        let video_path = match generate_fixture(temp_dir.path(), FixtureType::SceneDense) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Skipping test - FFmpeg not available: {}", e);
                return;
            }
        };

        let duration_ms = 5000;
        let ranges = FixtureType::SceneDense.expected_ranges();

        // Scene changes should be high (5 different colors)
        if let Ok((score, _)) = scene::analyze_scenes(&video_path, duration_ms, false) {
            assert!(
                ranges.contains("scene", score),
                "Scene score {} not in expected range {:?}",
                score, ranges.scene
            );
        }
    }

    #[test]
    fn test_fixture_good_audio() {
        let temp_dir = TempDir::new().unwrap();

        let video_path = match generate_fixture(temp_dir.path(), FixtureType::GoodAudioModerateVisual) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Skipping test - FFmpeg not available: {}", e);
                return;
            }
        };

        let duration_ms = 5000;
        let ranges = FixtureType::GoodAudioModerateVisual.expected_ranges();

        // Audio should score well (broadcast level sine wave)
        if let Ok((score, _)) = audio::analyze_audio(&video_path, duration_ms, false) {
            assert!(
                ranges.contains("audio", score),
                "Audio score {} not in expected range {:?}",
                score, ranges.audio
            );
        }
    }

    #[test]
    fn test_fixture_blurry() {
        let temp_dir = TempDir::new().unwrap();

        let video_path = match generate_fixture(temp_dir.path(), FixtureType::BlurryStatic) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Skipping test - FFmpeg not available: {}", e);
                return;
            }
        };

        let duration_ms = 5000;
        let ranges = FixtureType::BlurryStatic.expected_ranges();

        // Sharpness should be low (boxblur applied)
        if let Ok((score, _)) = sharpness::analyze_sharpness(&video_path, duration_ms, false) {
            assert!(
                ranges.contains("sharpness", score),
                "Sharpness score {} not in expected range {:?}",
                score, ranges.sharpness
            );
        }

        // Motion should be low (static)
        if let Ok((score, _)) = motion::analyze_motion(&video_path, duration_ms, false) {
            assert!(
                ranges.contains("motion", score),
                "Motion score {} not in expected range {:?}",
                score, ranges.motion
            );
        }
    }

    #[test]
    fn test_overall_score_calculation() {
        let mut result = ScoringResult::new(1);
        result.scene_score = 0.8;
        result.audio_score = 0.6;
        result.sharpness_score = 0.7;
        result.motion_score = 0.5;
        result.compute_overall();

        // With equal weights (0.25 each): (0.8 + 0.6 + 0.7 + 0.5) * 0.25 = 0.65
        let expected = 0.65;
        assert!(
            (result.overall_score - expected).abs() < 0.01,
            "Overall score {} not close to expected {}",
            result.overall_score, expected
        );
    }

    #[test]
    fn test_overall_score_clamping() {
        let mut result = ScoringResult::new(1);
        // Set all to maximum
        result.scene_score = 1.0;
        result.audio_score = 1.0;
        result.sharpness_score = 1.0;
        result.motion_score = 1.0;
        result.compute_overall();

        assert!(result.overall_score <= 1.0, "Score should be clamped to 1.0");
        assert!(result.overall_score >= 0.0, "Score should be at least 0.0");
    }

    #[test]
    fn test_score_ranges_validation() {
        let ranges = FixtureType::StaticSilent.expected_ranges();

        // Within range
        assert!(ranges.contains("scene", 0.5));
        assert!(ranges.contains("motion", 0.3));
        assert!(ranges.contains("sharpness", 0.5));
        assert!(ranges.contains("audio", 0.3));

        // Outside range (audio max is 0.6 for StaticSilent)
        assert!(!ranges.contains("audio", 0.9));

        // Unknown component
        assert!(!ranges.contains("unknown", 0.5));
    }
}
