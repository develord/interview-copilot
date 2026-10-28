<#
  Generates a small *synthetic* smoke corpus with the Windows text-to-speech voices, to check
  the STT pipeline end to end:

    powershell -ExecutionPolicy Bypass -File scripts/make-smoke-corpus.ps1
    cd src-tauri
    cargo run --features bench --bin stt-bench -- --provider assemblyai --manifest ../corpus/smoke/manifest.json

  Saved as UTF-8 *with BOM*: Windows PowerShell 5.1 reads BOM-less scripts as ANSI, which
  garbled the accented references (and the synthesized speech) in the first version.

  It is NOT the evaluation corpus: TTS audio is far cleaner than a real call. The real corpus
  (~150 utterances recorded through loopback and microphone, with every speaker's consent) uses
  the same manifest format.
#>
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Speech

$out = Join-Path $PSScriptRoot "..\corpus\smoke"
New-Item -ItemType Directory -Force $out | Out-Null

# French voice reading English terms reproduces the hard case: code-switching with a French accent.
$items = @(
  @{ id = "mix-01"; category = "mixed"; voice = "fr-FR"; text = "Comment tu gères le state management dans Vue ?"; terms = @("state management", "Vue") },
  @{ id = "mix-02"; category = "mixed"; voice = "fr-FR"; text = "Tu peux m'expliquer la différence entre Promise.all et Promise.allSettled ?"; say = "Tu peux m'expliquer la différence entre Promise point all et Promise point all settled ?"; terms = @("Promise.all", "Promise.allSettled") },
  @{ id = "mix-03"; category = "mixed"; voice = "fr-FR"; text = "Est-ce que tu as déjà travaillé avec Kubernetes et Kafka ?"; terms = @("Kubernetes", "Kafka") },
  @{ id = "mix-04"; category = "mixed"; voice = "fr-FR"; text = "C'est quoi la différence entre le server side rendering et le client side rendering ?"; terms = @("server side rendering", "client side rendering") },
  @{ id = "mix-05"; category = "mixed"; voice = "fr-FR"; text = "Comment tu mettrais en place une pipeline de CI CD avec GitHub Actions ?"; terms = @("GitHub Actions") },
  @{ id = "mix-06"; category = "mixed"; voice = "fr-FR"; text = "Pourquoi utiliser Redis comme cache devant PostgreSQL ?"; terms = @("Redis", "PostgreSQL") },
  @{ id = "fr-01"; category = "fr"; voice = "fr-FR"; text = "Bonjour, comment ça va aujourd'hui ?"; terms = @() },
  @{ id = "fr-02"; category = "fr"; voice = "fr-FR"; text = "Pourquoi souhaitez-vous quitter votre poste actuel ?"; terms = @() },
  @{ id = "fr-03"; category = "fr"; voice = "fr-FR"; text = "Parlez-moi d'une décision technique difficile que vous avez prise."; terms = @() },
  @{ id = "en-01"; category = "en"; voice = "en-US"; text = "Can you introduce yourself and explain your experience with Vue?"; terms = @("Vue") },
  @{ id = "en-02"; category = "en"; voice = "en-US"; text = "How would you design a URL shortener that handles millions of requests?"; terms = @("URL shortener") },
  @{ id = "en-03"; category = "en"; voice = "en-US"; text = "What is a closure in JavaScript?"; terms = @("closure", "JavaScript") }
)

$format = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000, [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen, [System.Speech.AudioFormat.AudioChannel]::Mono)
$manifest = @()
foreach ($item in $items) {
  $synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
  $voice = $synth.GetInstalledVoices() | Where-Object { $_.VoiceInfo.Culture.Name -eq $item.voice } | Select-Object -First 1
  if (-not $voice) { throw "No installed $($item.voice) voice" }
  $synth.SelectVoice($voice.VoiceInfo.Name)
  $file = "$($item.id).wav"
  $synth.SetOutputToWaveFile((Join-Path $out $file), $format)
  # `say` spells out what the synthesizer cannot read (e.g. "Promise.all"); `text` is the reference.
  $spoken = if ($item.say) { $item.say } else { $item.text }
  $synth.Speak($spoken)
  $synth.Dispose()
  $manifest += [ordered]@{ id = $item.id; file = $file; category = $item.category; reference = $item.text; terms = $item.terms }
}

$json = [ordered]@{ utterances = $manifest } | ConvertTo-Json -Depth 4
[IO.File]::WriteAllText((Join-Path $out "manifest.json"), $json, (New-Object System.Text.UTF8Encoding $false))
Write-Output "Wrote $($items.Count) utterances to $out"
