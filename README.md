# DeckLock

Tela de bloqueio personalizável para Wayland, com teclado virtual embutido,
imagens/vídeos de fundo e suporte opcional a controle.

Esta branch contém a **primeira implementação Rust + GTK4**, em desenvolvimento.
A versão Python continua no repositório como referência. O preview foi observado
no Hyprland; bloqueio e hotplug foram testados num compositor Wayland simulado.
A autenticação PAM na sessão real e a paridade completa do controle ainda precisam
de validação. Consulte [o estado da migração](docs/rust-migration.md).

## Executar o preview

Dependências de desenvolvimento: Rust 1.93+, GTK4 4.12+, GStreamer com bibliotecas
de desenvolvimento, plugins base/good e GL, Linux-PAM, pkg-config e
gtk4-layer-shell 1.3+. Para o bootstrap local, também Meson, Ninja, compilador C,
Wayland e wayland-protocols. Os codecs de vídeo dependem dos plugins instalados.

Com a biblioteca nativa instalada pela distribuição:

```sh
cargo run -- --preview --locale pt-BR --keyboard
```

Se gtk4-layer-shell não estiver disponível, compile-a localmente:

```sh
scripts/bootstrap-native
scripts/cargo-local run -- --preview --locale pt-BR --keyboard
```

O bootstrap verifica o SHA-256 do arquivo baixado e instala apenas em `.deps/`.
O wrapper `cargo-local` configura os caminhos da biblioteca local. Sem argumentos,
o programa também abre preview. `--preview-fullscreen` preenche o monitor;
Escape esconde o teclado aberto e, pressionado novamente, fecha a prévia.

**Preview nunca autentica nem executa ações de energia.** Ele ignora o socket de
controle salvo na configuração; captura de controle só acontece se você passar
`--controller` (socket padrão) ou `--controller-socket /caminho/do/socket`
explicitamente e abrir o teclado.

```sh
scripts/cargo-local run -- --preview --background /caminho/video.webm
scripts/cargo-local run -- --preview --theme themes/contrast --locale en-US
scripts/cargo-local run -- --check-config --config config.example.toml
```

O bloqueio real exige `--lock` e suporte do compositor a `ext-session-lock-v1`.
Wayland por si só não garante esse suporte. O programa não implementa X11.
Não substitua seu bloqueador já configurado antes de validar a versão Rust no seu
ambiente. SIGTERM/SIGINT e fechamento de janela não pedem desbloqueio; uma saída
inesperada pode deixar a sessão bloqueada, conforme a política do compositor.

## Testar o controle e o visual Python

```sh
scripts/cargo-local run -- --preview --locale pt-BR --preview-fullscreen --controller
```

Com essa janela ativa, o atalho existente `deck-osk --toggle` alterna o teclado
embutido. `--controller` registra temporariamente `scc/deck-lock.pid`; outra
instância viva não é substituída. O daemon sc-controller continua externo.
Fechar o teclado libera a captura. Preview sem `--controller` continua sendo o
modo de teste com mouse/teclado, sem capturar o controle.

O tema padrão reproduz a disposição do Python. Para importar também suas cores
do sc-controller e um fundo da pasta `~/.config/midias/bloqueio`:

```sh
scripts/import-python-theme
scripts/cargo-local run -- --preview --locale pt-BR --preview-fullscreen --controller --theme ~/.config/decklock/themes/python
```

A importação usa Python uma vez para gerar CSS/TOML editáveis; o Rust lê esses
arquivos diretamente. Executar o importador novamente sobrescreve o tema gerado.
O importador escolhe o primeiro fundo por nome; edite `background` no TOML para
escolher outro. O teclado virtual usa a geometria do SVG original e lê o primeiro grupo do mapa
GDK na abertura (incluindo níveis Shift/AltGr). Mudanças de grupo/layout durante
a execução ainda exigem reabrir. `system_keyboard = false` na configuração usa
o mapa brasileiro embutido.

## Personalização e idiomas

Copie [config.example.toml](config.example.toml) para
`~/.config/decklock/config.toml`, ou use `--config`. Temas são diretórios com
`theme.toml` e `style.css`. O tema padrão está embutido no binário; os diretórios
em `themes/` podem ser copiados e editados sem recompilar.

- [Guia de temas](docs/themes.md): CSS, disposição, tamanhos e mídias.
- [Guia de tradução](docs/i18n.md): catálogos Fluent, português/inglês e novos idiomas.
- Plugins e Lua ficam para uma etapa posterior; temas não executam scripts.

Mouse e teclado não dependem de sc-controller. A integração opcional usa o daemon
externo por socket Unix. `SIGUSR1` alterna o teclado da janela ativa.

## Verificação

```sh
scripts/cargo-local test --locked
scripts/cargo-local fmt --all -- --check
scripts/cargo-local clippy --locked --all-targets -- -D warnings
scripts/cargo-local build --locked
scripts/cargo-local run --locked --example preview_check
scripts/bootstrap-native --tests
python3 scripts/test-lock-isolated.py
```

O exemplo `preview_check` usa apenas uma janela de preview e dados fictícios.
O teste de protocolo usa **um socket temporário próprio**, nunca o socket Wayland
da sessão em uso. Ele não fornece senhas ao PAM.
