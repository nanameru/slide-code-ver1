# ハードウェア統合提案書

## 概要
Slide CLIプロジェクトを音声入力対応のハードウェアエージェントに拡張する技術的設計書

## アーキテクチャ設計

### 1. 音声入力モジュール (`slide-rs/voice-input/`)

```rust
// slide-rs/voice-input/src/lib.rs

use anyhow::Result;
use tokio::sync::mpsc;

pub enum VoiceInputEvent {
    TranscriptionReady(String),
    ListeningStarted,
    ListeningStopped,
    Error(String),
}

pub struct VoiceInputManager {
    tx: mpsc::Sender<VoiceInputEvent>,
}

impl VoiceInputManager {
    pub fn new() -> (Self, mpsc::Receiver<VoiceInputEvent>) {
        let (tx, rx) = mpsc::channel(100);
        (Self { tx }, rx)
    }

    pub async fn start_listening(&mut self) -> Result<()> {
        // TODO: Implement actual async operations:
        // 1. マイクからオーディオストリームを取得 (async)
        // 2. Whisper API または Google Speech-to-Text に送信 (async)
        // 3. トランスクリプションを取得 (async)
        // 4. VoiceInputEvent::TranscriptionReady で送信 (async)

        self.tx.send(VoiceInputEvent::ListeningStarted).await
            .map_err(|_| anyhow!("Failed to send listening started event"))?;

        // Placeholder for actual implementation
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        Ok(())
    }
}
```

### 2. ハードウェアインターフェース (`slide-rs/hardware-interface/`)

```rust
// slide-rs/hardware-interface/src/lib.rs

/// 物理ボタン、マイク、LED などのハードウェア抽象化
pub trait HardwareDevice {
    fn initialize(&mut self) -> Result<()>;
    fn read_event(&mut self) -> Result<HardwareEvent>;
}

pub enum HardwareEvent {
    ButtonPressed { button_id: u8 },
    VoiceDetected,
    Timeout,
}

// Raspberry Pi GPIO 対応例
#[cfg(feature = "raspberry-pi")]
pub mod raspberry_pi {
    use rppal::gpio::{Gpio, InputPin, Level};
    
    pub struct RaspberryPiInterface {
        record_button: InputPin,
    }
    
    impl RaspberryPiInterface {
        pub fn new() -> Result<Self> {
            let gpio = Gpio::new()?;
            let pin_number = std::env::var("RECORD_BUTTON_GPIO_PIN")
                .unwrap_or_else(|_| "17".to_string())
                .parse::<u8>()
                .map_err(|_| anyhow!("Invalid GPIO pin number"))?;
            let record_button = gpio.get(pin_number)?.into_input_pullup();
            Ok(Self { record_button })
        }
        
        pub fn is_record_button_pressed(&self) -> bool {
            self.record_button.read() == Level::Low
        }
    }
}
```

### 3. 統合コンポーネント

#### 3.1 音声認識プロバイダー

**オプション A: OpenAI Whisper API**
```rust
use reqwest::Client;

pub async fn transcribe_audio_whisper(
    audio_data: Vec<u8>,
    api_key: &str,
) -> Result<String> {
    let client = Client::new();
    let form = multipart::Form::new()
        .part("file", multipart::Part::bytes(audio_data)
            .file_name("audio.wav")
            .mime_str("audio/wav")?)
        .text("model", "whisper-1");
    
    let response = client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .await?;
    
    let result: WhisperResponse = response.json().await?;
    Ok(result.text)
}
```

**オプション B: ローカル音声認識 (Vosk)**
```rust
// Vosk を使用したオフライン音声認識
use vosk::{Model, Recognizer};

pub struct LocalVoiceRecognizer {
    model: Model,
}

impl LocalVoiceRecognizer {
    pub fn new(model_path: &str) -> Result<Self> {
        let model = Model::new(model_path)?;
        Ok(Self { model })
    }
    
    pub fn transcribe(&self, audio_data: &[i16]) -> Result<String> {
        let mut recognizer = Recognizer::new(&self.model, 16000.0)?;
        recognizer.accept_waveform(audio_data);
        let result = recognizer.final_result();
        Ok(result.text)
    }
}
```

#### 3.2 マイク入力キャプチャ

```rust
use cpal::{Device, Stream, StreamConfig};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub struct MicrophoneCapture {
    device: Device,
    config: StreamConfig,
}

impl MicrophoneCapture {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let device = host.default_input_device()
            .ok_or_else(|| anyhow!("No input device available"))?;
        let config = device.default_input_config()?.into();
        
        Ok(Self { device, config })
    }
    
    pub fn start_recording(&self, tx: mpsc::Sender<Vec<f32>>) -> Result<Stream> {
        let stream = self.device.build_input_stream(
            &self.config,
            move |data: &[f32], _: &_| {
                let _ = tx.try_send(data.to_vec());
            },
            |err| eprintln!("Audio stream error: {}", err),
            None,
        )?;
        
        stream.play()?;
        Ok(stream)
    }
}
```

### 4. 既存コードへの統合ポイント

#### 4.1 `slide-rs/cli/src/main.rs` への統合

```rust
use slide_voice_input::{VoiceInputManager, VoiceInputEvent};
use slide_hardware_interface::HardwareDevice;

#[tokio::main]
async fn main() -> Result<()> {
    // 既存の初期化コード...
    
    // 音声入力の初期化
    let (mut voice_manager, mut voice_rx) = VoiceInputManager::new();
    
    // ハードウェアインターフェースの初期化（オプション）
    #[cfg(feature = "hardware")]
    let mut hardware = RaspberryPiInterface::new()?;
    
    // イベントループ
    tokio::spawn(async move {
        loop {
            #[cfg(feature = "hardware")]
            if hardware.is_record_button_pressed() {
                voice_manager.start_listening().await?;
            }
            
            // 音声イベントを処理
            if let Some(event) = voice_rx.recv().await {
                match event {
                    VoiceInputEvent::TranscriptionReady(text) => {
                        // 既存の conversation に送信
                        conversation.submit(protocol::Op::UserInput {
                            items: vec![protocol::InputItem::Text { text }],
                        }).await?;
                    }
                    VoiceInputEvent::Error(err) => {
                        eprintln!("Voice input error: {}", err);
                    }
                    _ => {}
                }
            }
        }
    });
    
    // 既存のメインループ...
}
```

#### 4.2 `slide-rs/tui/` への音声フィードバック統合

```rust
// TUI に音声録音インジケータを追加
pub struct VoiceRecordingWidget {
    is_recording: bool,
}

impl Widget for VoiceRecordingWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.is_recording {
            let text = "🎤 録音中...";
            // ratatui で描画
        }
    }
}
```

## 実装ステップ

### Phase 1: 基本音声入力 (1-2週間)
- [ ] マイク入力キャプチャ実装 (`cpal` crate)
- [ ] Whisper API 統合
- [ ] 音声 → テキスト → 既存パイプライン接続

### Phase 2: ハードウェアインターフェース (1週間)
- [ ] GPIO ボタン入力対応 (Raspberry Pi)
- [ ] LED フィードバック実装
- [ ] ハードウェアイベントループ

### Phase 3: ローカル音声認識 (オプション) (2-3週間)
- [ ] Vosk または Whisper.cpp 統合
- [ ] オフライン動作モード
- [ ] 音声モデルのダウンロード管理

### Phase 4: UI/UX 改善 (1週間)
- [ ] TUI への音声インジケータ追加
- [ ] 音声コマンドショートカット
- [ ] エラーハンドリング強化

## 必要な依存関係

```toml
# Cargo.toml に追加
[dependencies]
# 音声入力
cpal = "0.15"                    # マイク入力
hound = "3.5"                    # WAV フォーマット
whisper-rs = "0.10"              # ローカル Whisper (オプション)
vosk = "0.3"                     # ローカル音声認識 (オプション)

# ハードウェア (feature-gated)
rppal = { version = "0.14", optional = true }  # Raspberry Pi GPIO

[features]
default = []
hardware = ["rppal"]
local-stt = ["vosk"]  # ローカル音声認識
```

## ハードウェア要件

### 最小構成
- USB マイク または 内蔵マイク
- 任意の Linux/macOS コンピュータ

### 推奨構成 (専用ハードウェアエージェント)
- **Raspberry Pi 4** (4GB RAM 以上)
- **USB マイク** または I2S マイク
- **物理ボタン** (GPIO 接続)
- **LED インジケータ** (状態表示用)
- **スピーカー** (音声フィードバック用、オプション)

## セキュリティとサンドボックス

音声入力も既存のサンドボックス機構を通過します:

```rust
// 音声からの入力も同じ承認フローを通る
pub async fn process_voice_command(
    text: String,
    executor: &mut SandboxedExecutor,
) -> Result<()> {
    // 音声入力を解析
    let command = parse_voice_command(&text)?;
    
    // 既存のサンドボックスで実行
    let params = ExecParams {
        command,
        // ... 既存のセキュリティポリシー適用
    };
    
    executor.execute(params).await?;
    Ok(())
}
```

## 使用例

### 例1: 音声でファイル編集
```
[ユーザー] 🎤 「README.mdの5行目を'Hello World'に変更して」
[エージェント] ✅ パッチを適用しました
```

### 例2: 音声でコマンド実行
```
[ユーザー] 🎤 「カレントディレクトリのファイル一覧を表示して」
[エージェント] 🤖 実行コマンド: ls -la
[承認プロンプト] このコマンドを実行しますか？ [y/n]
```

### 例3: ハードウェアボタンによる操作
```
[物理ボタン押下] → 録音開始
[ユーザー音声] 「テストを実行して」
[ボタン離す] → 録音停止・送信
[LED点滅] → 処理中
[LED点灯] → 完了
```

## 既存のサンドボックス機構との互換性

✅ **完全互換**: 音声入力は単なる入力ソースの変更なので、既存の以下の機能はそのまま動作します:

- ✅ macOS Seatbelt サンドボックス
- ✅ Linux Landlock サンドボックス
- ✅ 承認フロー (ApprovalRequest)
- ✅ ファイル操作制限
- ✅ コマンド安全性チェック

## まとめ

このプロジェクトは**モジュラーな設計**と**イベント駆動アーキテクチャ**により、
**ハードウェア統合が技術的に十分可能**です。

### メリット
- 🎤 ハンズフリー操作
- 🤖 音声でAIエージェント制御
- 🔒 既存のサンドボックス保護を維持
- 🛠️ モジュール追加のみで実装可能

### 実装難易度
- **基本音声入力**: 中程度 (1-2週間)
- **ハードウェア統合**: 低〜中程度 (GPIO は簡単)
- **ローカル音声認識**: 高 (モデル統合が複雑)

技術的には**完全に実現可能**で、既存の構造を壊さずに拡張できます！
