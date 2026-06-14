//! SLFAM Enrollment CLI
//!
//! Command-line tool for enrolling users for facial authentication.

use clap::{Parser, Subcommand};
use slfam::camera::{Camera, CameraType, enumerate_cameras};
use slfam::config::Config;
use slfam::crypto::{KeyDerivation, TpmKeyDerivation};
use slfam::detection::FaceDetectionPipeline;
use slfam::embedding::EmbeddingGenerator;
use slfam::error::Result;
use slfam::liveness::LivenessAnalyzer;
use slfam::template::{Template, TemplateMetadata, TemplateStore};
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "slfam-enroll")]
#[command(author, version, about = "SLFAM Face Enrollment Tool", long_about = None)]
struct Cli {
    /// Configuration file path
    #[arg(short, long, default_value = "/etc/slfam/config.toml")]
    config: PathBuf,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Enroll a new user
    Enroll {
        /// Username to enroll
        #[arg(short, long)]
        user: String,

        /// Number of face samples to capture
        #[arg(short, long, default_value = "5")]
        samples: u32,

        /// Skip liveness detection (development only)
        #[arg(long)]
        skip_liveness: bool,

        /// Force re-enrollment (overwrite existing)
        #[arg(short, long)]
        force: bool,
    },

    /// Remove a user's enrollment
    Remove {
        /// Username to remove
        #[arg(short, long)]
        user: String,
    },

    /// List enrolled users
    List,

    /// Update an existing enrollment
    Update {
        /// Username to update
        #[arg(short, long)]
        user: String,

        /// Number of additional samples to capture
        #[arg(short, long, default_value = "3")]
        samples: u32,
    },

    /// Verify enrollment works
    Verify {
        /// Username to verify
        #[arg(short, long)]
        user: String,
    },

    /// Show camera information
    CameraInfo,

    /// Test face detection
    Test {
        /// Number of frames to test
        #[arg(short, long, default_value = "10")]
        frames: u32,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load configuration
    let config = if cli.config.exists() {
        Config::load(&cli.config)?
    } else {
        println!("Warning: Config file not found, using defaults");
        Config::default()
    };

    match cli.command {
        Commands::Enroll {
            user,
            samples,
            skip_liveness,
            force,
        } => {
            enroll_user(&config, &user, samples, skip_liveness, force, cli.verbose)?;
        }
        Commands::Remove { user } => {
            remove_user(&config, &user, cli.verbose)?;
        }
        Commands::List => {
            list_users(&config, cli.verbose)?;
        }
        Commands::Update { user, samples } => {
            update_user(&config, &user, samples, cli.verbose)?;
        }
        Commands::Verify { user } => {
            verify_user(&config, &user, cli.verbose)?;
        }
        Commands::CameraInfo => {
            show_camera_info(cli.verbose)?;
        }
        Commands::Test { frames } => {
            test_detection(&config, frames, cli.verbose)?;
        }
    }

    Ok(())
}

fn enroll_user(
    config: &Config,
    user: &str,
    samples: u32,
    skip_liveness: bool,
    force: bool,
    verbose: bool,
) -> Result<()> {
    println!("SLFAM Face Enrollment");
    println!("=====================");
    println!("User: {}", user);
    println!("Samples to capture: {}", samples);
    println!();

    // Initialize template store
    let mut store = TemplateStore::new(&config.general.template_dir)?;

    // Check if already enrolled
    if store.exists(user) && !force {
        println!("Error: User '{}' is already enrolled.", user);
        println!("Use --force to overwrite.");
        return Ok(());
    }

    // Initialize key derivation
    let key_path = std::path::Path::new(&config.general.template_dir).join(".key");
    let key_derivation = TpmKeyDerivation::new(&key_path, config.security.use_tpm)?;
    let key = key_derivation.derive_key(user, b"slfam-auth")?;

    if verbose {
        println!(
            "Using {}: {}",
            if key_derivation.using_tpm() { "TPM" } else { "file-based" },
            "key derivation"
        );
    }

    // Open camera
    println!("Opening camera...");
    let mut camera = open_camera(config, verbose)?;
    camera.start_streaming()?;

    if verbose {
        println!("Camera: {} (IR: {})", camera.device_path(), camera.is_ir());
    }

    // Initialize detection pipeline
    println!("Loading detection models...");
    let detection_pipeline = FaceDetectionPipeline::new(
        &config.general.model_dir,
        config.detection.clone(),
    )?;

    // Initialize embedding generator
    let embedding_model = std::path::Path::new(&config.general.model_dir)
        .join(&config.detection.embedding_model);
    let embedding_gen = EmbeddingGenerator::load(&embedding_model)?;

    // Initialize liveness analyzer
    let mut liveness = LivenessAnalyzer::new(config.liveness.clone(), camera.is_ir());

    // Collect samples
    println!();
    println!("Position your face in front of the camera.");
    println!("Look straight ahead, with good lighting.");
    println!();

    let mut embeddings = Vec::new();
    let mut current_sample = 0;

    while current_sample < samples {
        print!("\rCapturing sample {}/{}...", current_sample + 1, samples);
        io::stdout().flush()?;

        // Clear liveness buffer
        liveness.reset();

        // Collect frames for liveness
        let required_frames = config.liveness.optical_flow_frames as usize;
        let mut valid_frames = 0;
        let mut last_processed = None;

        while valid_frames < required_frames {
            let frame = camera.capture_frame()?;

            match detection_pipeline.process_frame(&frame) {
                Ok(processed) => {
                    let _ = liveness.add_frame(&frame, processed.landmarks.clone());
                    last_processed = Some(processed);
                    valid_frames += 1;
                }
                Err(e) => {
                    if verbose {
                        eprintln!("\nDetection failed: {}", e);
                    }
                }
            }
        }

        // Check liveness
        if !skip_liveness {
            match liveness.analyze() {
                Ok(result) if result.is_live => {
                    if verbose {
                        println!(" Liveness OK ({:.0}%)", result.confidence * 100.0);
                    }
                }
                Ok(result) => {
                    println!(" Liveness failed ({:.0}%)", result.confidence * 100.0);
                    println!("Please ensure you are a live person and try again.");
                    continue;
                }
                Err(e) => {
                    if verbose {
                        eprintln!(" Liveness error: {}", e);
                    }
                    continue;
                }
            }
        }

        // Generate embedding
        if let Some(processed) = last_processed {
            if let Some(aligned) = processed.aligned {
                match embedding_gen.generate(&aligned) {
                    Ok(embedding) => {
                        embeddings.push(embedding);
                        current_sample += 1;
                        println!(" OK");
                    }
                    Err(e) => {
                        if verbose {
                            eprintln!(" Embedding failed: {}", e);
                        }
                    }
                }
            }
        }

        // Brief pause between samples
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    camera.stop_streaming()?;

    // Create and save template
    println!();
    println!("Creating template...");

    let mut metadata = TemplateMetadata::new();
    metadata.extra.insert("enrolled_by".to_string(), "slfam-enroll".to_string());

    let template = Template::new(user.to_string(), embeddings, Some(metadata));
    store.save(&template, &key)?;

    println!("✓ User '{}' enrolled successfully!", user);
    println!("  {} face samples stored.", samples);

    Ok(())
}

fn remove_user(config: &Config, user: &str, _verbose: bool) -> Result<()> {
    let mut store = TemplateStore::new(&config.general.template_dir)?;

    if !store.exists(user) {
        println!("User '{}' is not enrolled.", user);
        return Ok(());
    }

    print!("Remove enrollment for '{}'? [y/N] ", user);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim().to_lowercase() == "y" {
        store.delete(user)?;
        println!("✓ User '{}' removed.", user);
    } else {
        println!("Cancelled.");
    }

    Ok(())
}

fn list_users(config: &Config, _verbose: bool) -> Result<()> {
    let store = TemplateStore::new(&config.general.template_dir)?;
    let users = store.list_users()?;

    if users.is_empty() {
        println!("No users enrolled.");
    } else {
        println!("Enrolled users ({}):", users.len());
        for user in &users {
            println!("  - {}", user);
        }
    }

    Ok(())
}

fn update_user(config: &Config, user: &str, samples: u32, _verbose: bool) -> Result<()> {
    let mut store = TemplateStore::new(&config.general.template_dir)?;

    if !store.exists(user) {
        println!("User '{}' is not enrolled.", user);
        println!("Use 'enroll' command to create a new enrollment.");
        return Ok(());
    }

    // Load existing template
    let key_path = std::path::Path::new(&config.general.template_dir).join(".key");
    let key_derivation = TpmKeyDerivation::new(&key_path, config.security.use_tpm)?;
    let key = key_derivation.derive_key(user, b"slfam-auth")?;
    
    let template = store.load(user, &key)?;

    println!("Updating enrollment for '{}'", user);
    println!("Current samples: {}", template.embeddings().len());
    println!("Adding: {} new samples", samples);

    // Similar capture loop as enroll (abbreviated)
    // In full implementation, would reuse code from enroll_user

    println!("(Update capture not fully implemented in this example)");

    Ok(())
}

fn verify_user(config: &Config, user: &str, verbose: bool) -> Result<()> {
    let mut store = TemplateStore::new(&config.general.template_dir)?;

    if !store.exists(user) {
        println!("User '{}' is not enrolled.", user);
        return Ok(());
    }

    println!("Verifying enrollment for '{}'...", user);

    // Load template
    let key_path = std::path::Path::new(&config.general.template_dir).join(".key");
    let key_derivation = TpmKeyDerivation::new(&key_path, config.security.use_tpm)?;
    let key = key_derivation.derive_key(user, b"slfam-auth")?;

    let template = store.load(user, &key)?;

    println!("✓ Template loaded successfully");
    println!("  Embeddings: {}", template.embeddings().len());
    println!("  Created: {}", template.metadata().created_at);
    println!("  Auth count: {}", template.metadata().auth_count);

    // Verify embedding integrity
    for (i, emb) in template.embeddings().iter().enumerate() {
        let norm: f32 = emb.data().iter().map(|x| x * x).sum::<f32>().sqrt();
        if verbose {
            println!("  Embedding {}: dim={}, norm={:.4}", i, emb.dim(), norm);
        }
        if (norm - 1.0).abs() > 0.01 {
            println!("  Warning: Embedding {} not properly normalized", i);
        }
    }

    println!("✓ Verification complete");

    Ok(())
}

fn show_camera_info(verbose: bool) -> Result<()> {
    println!("Camera Information");
    println!("==================");

    let devices = enumerate_cameras();
    if devices.is_empty() {
        println!("No video devices found.");
    } else {
        for device in &devices {
            println!();
            println!("Device: {}", device.path.display());
            println!("  Name: {}", device.name);
            println!("  Type: {}", device.camera_type);
            println!("  Can capture: {}", device.capabilities.video_capture);
            
            if verbose {
                println!("  Driver: {}", device.driver);
                println!("  Card: {}", device.card);
            }
        }
    }

    Ok(())
}

fn test_detection(config: &Config, frames: u32, verbose: bool) -> Result<()> {
    println!("Testing face detection...");

    let mut camera = open_camera(config, verbose)?;
    camera.start_streaming()?;

    let detection_pipeline = FaceDetectionPipeline::new(
        &config.general.model_dir,
        config.detection.clone(),
    )?;

    let mut successes = 0;
    let mut _failures = 0;

    for i in 0..frames {
        print!("\rFrame {}/{}: ", i + 1, frames);
        io::stdout().flush()?;

        match camera.capture_frame() {
            Ok(frame) => {
                match detection_pipeline.process_frame(&frame) {
                    Ok(result) => {
                        successes += 1;
                        print!("Face detected at ({:.0}, {:.0})",
                            result.face_bbox.x, result.face_bbox.y);
                    }
                    Err(e) => {
                        _failures += 1;
                        print!("No face: {}", e);
                    }
                }
            }
            Err(e) => {
                _failures += 1;
                print!("Capture failed: {}", e);
            }
        }
        println!();
    }

    camera.stop_streaming()?;

    println!();
    println!("Results: {}/{} frames with face detected", successes, frames);
    println!("Detection rate: {:.1}%", (successes as f32 / frames as f32) * 100.0);

    Ok(())
}

fn open_camera(config: &Config, verbose: bool) -> Result<Camera> {
    if verbose {
        println!("Opening camera...");
    }
    
    Camera::open(&config.camera, CameraType::Rgb)
        .map_err(|e| {
            if verbose {
                println!("Failed to open camera: {}", e);
            }
            e
        })
}
