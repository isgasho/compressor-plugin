use std::f32::consts::PI;

use crate::units::*;

//x, threshold, & width units are dB
//slope is: 1.0 / ratio - 1.0 (Computed ahead of time for performance)
fn reiss(x: f32, threshold: f32, width: f32, ratio: f32, slope: f32) -> f32 {
    let x_minus_threshold = x - threshold;
    if 2.0 * (x_minus_threshold).abs() <= width {
        x + slope * (x_minus_threshold + width / 2.0).powi(2) / (2.0 * width)
    } else if 2.0 * (x_minus_threshold) > width {
        threshold + (x_minus_threshold) / ratio
    } else {
        // if 2.0 * (x_minus_threshold) < -width
        x
    }
}

pub struct DecoupledPeakDetector {
    attack: f32,
    release: f32,
    env: f32,
    env2: f32,
}

impl DecoupledPeakDetector {
    pub fn new(attack: f32, release: f32, sample_rate: f32) -> DecoupledPeakDetector {
        let mut detector = DecoupledPeakDetector {
            attack: 0.0,
            release: 0.0,
            env: 0.0,
            env2: 0.0,
        };
        detector.update(attack, release, sample_rate);
        detector
    }

    pub fn process(&mut self, x: f32) -> f32 {
        self.env = x.max(self.release * self.env);
        self.env2 = self.attack * self.env2 + (1.0 - self.attack) * self.env;
        self.env2
    }

    pub fn process_smooth(&mut self, x: f32) -> f32 {
        self.env = x.max(self.release * self.env + (1.0 - self.release) * x);
        self.env2 = self.attack * self.env2 + (1.0 - self.attack) * self.env;

        self.env = if self.env.is_finite() { self.env } else { 1.0 };
        self.env2 = if self.env2.is_finite() {
            self.env2
        } else {
            1.0
        };
        self.env2
    }

    pub fn update(&mut self, attack: f32, release: f32, sample_rate: f32) {
        self.attack = (-1.0 * PI * 1000.0 / attack / sample_rate).exp();
        self.release = (-1.0 * PI * 1000.0 / release / sample_rate).exp();
    }
}

pub struct Compressor {
    envelope: f32,
    threshold: f32,
    knee: f32,
    ratio: f32,
    gain: f32,

    slope: f32,

    pre_smooth_gain: f32,
    decoupled_peak_detector: DecoupledPeakDetector,
    rms_size: f32,
    rms: AccumulatingRMS,
}

impl Compressor {
    pub fn new() -> Compressor {
        Compressor {
            envelope: 0.0,
            threshold: 0.0,
            knee: 0.0,
            ratio: 0.0,
            gain: 0.0,

            slope: 0.0,

            pre_smooth_gain: 0.0,
            decoupled_peak_detector: DecoupledPeakDetector::new(0.0, 0.0, 48000.0),

            rms_size: 0.0,
            rms: AccumulatingRMS::new(48000, 5.0, 192000),
        }
    }

    pub fn update_prams(
        &mut self,
        threshold: f32,
        knee: f32,
        pre_smooth: f32,
        rms_size: f32,
        ratio: f32,
        attack: f32,
        release: f32,
        gain: f32,
        sample_rate: f32,
    ) {
        //TODO don't update here unnecessarily
        self.ratio = ratio;
        self.gain = db_to_lin(gain);
        self.threshold = threshold;
        self.knee = knee;

        self.slope = 1.0 / self.ratio - 1.0;
        self.pre_smooth_gain = (-2.0 * PI * 1000.0 / pre_smooth / sample_rate).exp();
        self.decoupled_peak_detector
            .update(attack, release, sample_rate);

        if rms_size != self.rms_size {
            self.rms_size = rms_size;
            self.rms.resize(sample_rate as usize, self.rms_size)
        }
    }

    //To make detector_input from stereo:
    //detector_input = (input_l + input_r).abs() * 0.5
    //Returns attenuation multiplier
    pub fn process(&mut self, detector_input: f32) -> f32 {
        let mut detector_input = detector_input;
        if self.rms_size >= 1.0 {
            detector_input = self.rms.process(detector_input);
        }

        self.envelope = detector_input + self.pre_smooth_gain * (self.envelope - detector_input);

        self.envelope = if self.envelope.is_finite() {
            self.envelope
        } else {
            1.0
        };

        let db = lin_to_db(self.envelope);

        let mut cv = db - reiss(db, self.threshold, self.knee, self.ratio, self.slope);
        cv = db_to_lin(-self.decoupled_peak_detector.process_smooth(cv));
        if cv.is_finite() {
            cv
        } else {
            1.0
        }
    }
}
