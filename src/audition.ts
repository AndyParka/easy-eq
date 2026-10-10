import { coefficients, dbToGain, headroomDb, panMatrix, type Profile } from './core';

export class Audition {
  context: AudioContext | null = null;
  node: AudioWorkletNode | null = null;
  analyzer: AnalyserNode | null = null;
  volume: GainNode | null = null;
  media = new Audio();
  url: string | null = null;
  bins = new Float32Array(2048);
  level = 0.25;
  constructor() { this.media.loop = true; }
  async open(file: File, profile: Profile, enabled: boolean) {
    this.media.pause();
    if (!this.context) {
      const context = new AudioContext();
      try {
        await context.audioWorklet.addModule('/eq-worklet.js');
        this.node = new AudioWorkletNode(context, 'easy-eq-audition', { outputChannelCount: [2], channelCount: 2, channelCountMode: 'explicit' });
        this.analyzer = context.createAnalyser(); this.analyzer.fftSize = 4096; this.analyzer.smoothingTimeConstant = 0.8;
        this.volume = context.createGain(); this.volume.gain.value = this.level;
        context.createMediaElementSource(this.media).connect(this.node).connect(this.analyzer).connect(this.volume).connect(context.destination);
        this.context = context;
      } catch (error) { await context.close(); throw error; }
    }
    if (this.url) URL.revokeObjectURL(this.url);
    this.url = URL.createObjectURL(file); this.media.src = this.url;
    this.update(profile, enabled);
  }
  update(profile: Profile, enabled: boolean) {
    if (!this.context || !this.node) return;
    const rate = this.context.sampleRate;
    this.node.port.postMessage({
      ears: enabled ? [profile.ears.left.map(b => coefficients(b, rate)), profile.ears.right.map(b => coefficients(b, rate))] : [[], []],
      matrix: enabled ? panMatrix(profile.pan, profile.panMode) : [1, 0, 0, 1],
      gain: enabled ? dbToGain(profile.gainDb + headroomDb(profile, rate)) : 1,
    });
  }
  async toggle() {
    if (!this.context || !this.url) throw new Error('Choose an audio file first.');
    if (this.media.paused) { await this.context.resume(); await this.media.play(); }
    else this.media.pause();
  }
  setVolume(level: number) { this.level = level; this.volume?.gain.setTargetAtTime(level, this.context!.currentTime, 0.02); }
  spectrum() {
    if (!this.analyzer || this.media.paused) return null;
    this.analyzer.getFloatFrequencyData(this.bins); return this.bins;
  }
}
