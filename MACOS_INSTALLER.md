# CPU Limiter - Instalador macOS

Instruções para criar e instalar o CPU Limiter no macOS.

## Opção 1: Build e DMG Automático

```bash
chmod +x build_macos.sh
./build_macos.sh
```

Isso irá:
1. Compilar a aplicação em modo release
2. Criar um app bundle (.app)
3. Gerar um arquivo DMG pronto para distribuição

O DMG será criado em `dist/CPULimiter-0.1.0.dmg`

## Opção 2: Instalação Direta

Após executar o script de build:

```bash
chmod +x install_macos.sh
./install_macos.sh dist/CPULimiter-0.1.0.dmg
```

Ou instale manualmente:
1. Abra o DMG criado
2. Arraste "CPU Limiter.app" para a pasta Applications

## Opção 3: Homebrew Tap (Distribuição)

Para distribuir via Homebrew, você pode:

1. Criar um repositório Homebrew tap: `homebrew-cpu-limiter`
2. Usar este arquivo de fórmula:

```ruby
class CpuLimiter < Formula
  desc "CPU Limiter - Controle o uso de CPU"
  homepage "https://github.com/alexkads/cavalo-doido"
  version "0.1.0"

  on_macos do
    url "https://github.com/alexkads/cavalo-doido/releases/download/v0.1.0/CPULimiter-0.1.0.dmg"
    sha256 "HASH_DO_DMG"
  end

  def install
    app.install "CPU Limiter.app"
  end

  test do
    assert_path_exists "#{appdir}/CPU Limiter.app"
  end
end
```

Depois instalar com:
```bash
brew install alexkads/cpu-limiter/cpu-limiter
```

## Requisitos

- macOS 10.13 ou superior
- Acesso de administrador para instalar em `/Applications`

## Desinstalação

Basta arrastar "CPU Limiter.app" da pasta Applications para o Lixo.

## Troubleshooting

### "Não é possível abrir porque vem de desenvolvedor não identificado"

1. Abra Preferências de Segurança
2. Na aba "Geral", clique em "Abrir assim mesmo" para CPU Limiter.app

Ou execute no terminal:
```bash
sudo xattr -rd com.apple.quarantine /Applications/CPU\ Limiter.app
```

### Permissões

Se a aplicação não conseguir limitar CPU, pode ser necessário:
```bash
sudo chown root /Applications/CPU\ Limiter.app/Contents/MacOS/CPU\ Limiter
sudo chmod u+s /Applications/CPU\ Limiter.app/Contents/MacOS/CPU\ Limiter
```
