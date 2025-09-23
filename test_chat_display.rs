// 一時的なテストファイル：チャット表示確認用

use std::path::Path;
use std::process::Command;

// slide-rs/tui/src/history_cell.rsの内容を直接確認
fn main() {
    let file_path = "slide-rs/tui/src/history_cell.rs";
    
    println!("=== チャット表示修正の確認 ===\n");
    
    if Path::new(file_path).exists() {
        let output = Command::new("grep")
            .args(&["-A", "5", "-B", "2", "heading_span", file_path])
            .output()
            .expect("grepコマンドの実行に失敗");
            
        println!("📋 RoleLabel::heading_span()の実装:");
        println!("{}", String::from_utf8_lossy(&output.stdout));
        
        let output2 = Command::new("grep")
            .args(&["-A", "8", "-B", "2", "RoleLabel::User =>", file_path])
            .output()
            .expect("grepコマンドの実行に失敗");
            
        println!("\n📋 User メッセージの色設定:");
        println!("{}", String::from_utf8_lossy(&output2.stdout));
        
        println!("\n✅ 修正内容:");
        println!("- User: 'user'(cyan/bold) → '>'(dark_gray)");
        println!("- Assistant: 'assistant'(green/bold) → '・'(white)");
        println!("- User本文: default → DarkGray");
        println!("- Assistant本文: format_content_line (変更なし)");
        
    } else {
        println!("❌ ファイルが見つかりません: {}", file_path);
    }
}
