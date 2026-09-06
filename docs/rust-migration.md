# Estado da migração Rust

Data: 2026-09-06. Branch: `feat/rust-gtk4`.

Esta é a primeira base funcional Rust/GTK4 para Wayland. Ela não substitui ainda
todas as funções da referência Python. X11 foi removido do escopo e a execução de plugins/Lua foi
adiada por decisão do usuário; não há runtime de plugins disfarçado de tema.

## Implementado

- Preview padrão e `--lock` explícito; preview sem PAM/energia/captura automática.
- Temas externos CSS + layout TOML limitado e validado; dois temas de exemplo.
- Fluent com pt-BR/en-US, sobreposições externas e fallback.
- Campo de senha, mostrar/ocultar, teclado clicável, Shift/Caps virtual, acentos
  compostos e edição Unicode; entrada física via GTK.
- Relógio/data, avatar, modo ocioso, imagens e vídeos locais mudos/em loop.
- GStreamer GTK4 sink integrado ao binário, com GL Wayland quando disponível.
- PAM em helper separado, usuário pelo UID, entrada limitada, timeout, sem senha
  em argumentos/logs; só sucesso da tentativa atual autoriza desbloqueio.
- Superfícies de bloqueio por monitor com hotplug; SIGTERM/SIGINT não desbloqueiam.
- Cliente opcional sc-controller com socket/filas limitados, timeout de conexão,
  seleção/captura/liberação; uma conexão compartilhada entre monitores, direcionada
  à janela ativa. Sem Python para o funcionamento Rust de mouse/teclado.

## Evidência e limites

21 testes unitários/de integração cobrem configuração, Fluent, teclado, estado,
helper de autenticação simulado e protocolo de controle simulado.
`examples/preview_check.rs` verifica cliques GTK, Shift, acentos, exclusão Unicode,
envio sem autenticação, energia desativada e liberação do campo ao destruir janela.

Preview observado em Hyprland a 1920×1080, em português e inglês; campo de senha
testado com texto fictício; vídeo de teste de dois segundos observado após repetir.
Não foi usado nenhum segredo real. Não há benchmark comparativo com Python.

`scripts/test-lock-isolated.py` executa o binário contra o compositor simulado do
gtk4-layer-shell 1.3, com socket temporário próprio. Verifica aquisição, inclusão,
remoção e reinclusão de monitor; SIGTERM sem mensagem de desbloqueio; recusa de um
segundo bloqueador após a morte do primeiro. **Não é UAT de uma sessão real.**

Um erro de ciclo de vida foi reproduzido com GTK 4.22: a remoção de uma
ApplicationWindow de bloqueio acessava uma superfície já destruída pela biblioteca.
As janelas de bloqueio agora seguem o ciclo da biblioteca e não são registradas
em GtkApplication; o aplicativo é mantido vivo explicitamente. Preview usa janelas
normais. O teste de hotplug passou após a correção.

## Paridade ainda pendente

- Teste no controle físico: ergonomia, cursores, calibração, reconexão e recuperação
  de disputa de captura. A conexão não se reconecta automaticamente nesta versão.
- Efeito fantasma por proximidade e vibração do teclado Python; o Rust usa realce
  da tecla selecionada, com teclado sólido.
- Sincronização de layouts XKB, AltGr e layout SVG personalizado do teclado Python.
  A grade virtual inicial é fixa e inclui ç/acentos; o teclado físico usa GTK.
- Teclado Rust independente para digitar em outros aplicativos. `deck_osk.py`
  continua sendo a referência desse recurso.
- Validação PAM com a configuração real da distribuição, integração de suspensão
  e ações de energia reais; não foi instalada/configurada nenhuma delas.
- UAT de bloqueio em Hyprland/Sway reais, escalas/resoluções diversas e vídeo com
  decodificadores/hardware distintos. O protocolo só funciona onde o compositor
  expõe `ext-session-lock-v1`; não há promessa de suporte a todo desktop Wayland.
- Temas com organização arbitrária, recarga ao editar e API de plugins.

## Comandos de continuação

```sh
scripts/bootstrap-native --tests
scripts/cargo-local test --locked
scripts/cargo-local run -- --preview --locale pt-BR --keyboard --preview-fullscreen
scripts/cargo-local run --example preview_check
python3 scripts/test-lock-isolated.py
```

O código Python original não foi alterado pela migração. Não ativar o Rust como
bloqueador da sessão antes de UAT explícita no ambiente de destino.
