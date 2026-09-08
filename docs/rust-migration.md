# Estado da migração Rust

Data: 2026-09-06. Integração na branch principal `main`.

Rust/GTK4 é agora a implementação principal para Wayland. Os quatro módulos
Python foram retirados da árvore atual; permanecem no histórico em `7459bb1`.
Ainda existem diferenças funcionais em relação à referência Python. X11 foi removido do escopo e a execução de plugins/Lua foi
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

## Correção de integração e aparência

O preview inicial não ativava sc-controller nem registrava o PID esperado por
`deck-osk --toggle`, então o atalho abria o teclado Python separado. `--controller`
agora resolve o socket padrão e registra a compatibilidade com esse atalho.
`scripts/test-controller-shortcut.py` executa agora o launcher Rust
`--toggle-keyboard` com HOME temporário e daemon simulado, verificando
captura/liberação do teclado embutido e ausência de outro OSK. A compatibilidade
com o launcher Python foi verificada antes da retirada dos módulos.

O tema padrão retorna à disposição do Python: relógio/data, credenciais, energia
no canto superior e teclado inferior; modo mouse compacto e modo controle com
transparência por proximidade. `scripts/import-python-theme` converte cores e
fundo locais para um tema externo; nenhuma configuração do sc-controller é alterada.

O Shift virtual segue a referência Python: dois cliques em menos de 600 ms
travam o modificador, com indicação de Caps Lock; outro clique destrava.
O estado de toque agora controla a visibilidade dos pads: o evento neutro `(0, 0)`
enviado pelo daemon depois da liberação não recria um dedo no centro. O teste GTK
cobre ambos os pads, soltura independente e tentativa de pressionar uma tecla
após soltar. Os dois defeitos foram reproduzidos nos testes antes da correção.

## Evidência e limites

30 testes unitários/de integração cobrem configuração, Fluent, teclado, estado,
helper de autenticação simulado e protocolo de controle simulado.
`examples/preview_check.rs` verifica cliques GTK, Shift, acentos, exclusão Unicode,
envio sem autenticação, cliques de energia inertes no preview e liberação do campo ao destruir janela.

Previews Python/Rust comparados visualmente no monitor de 1280×800, nos modos
normal e teclado mouse, com fundo e cores importados. A composição original foi
restaurada; GTK3/GTK4 ainda têm pequenas diferenças de rasterização e métricas.
Preview anterior observado em Hyprland a 1920×1080, em português e inglês; campo de senha
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
- Vibração do teclado Python e validação no controle físico do efeito fantasma
  por proximidade, agora implementado no Rust.
- Atualização dinâmica e seleção de grupos XKB, e SVGs personalizados em execução.
  A geometria original foi incorporada. Na abertura, o teclado consulta o primeiro
  grupo GDK e seus níveis Shift/AltGr; há mapa brasileiro embutido como alternativa.
- Teclado Rust independente para digitar em outros aplicativos. A versão Python
  no histórico continua sendo a referência desse recurso.
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

O código Python original pode ser consultado no histórico (`7459bb1`). Não ativar o Rust como
bloqueador da sessão antes de UAT explícita no ambiente de destino.

Alt virtual: quando o mapa GDK tem somente dois níveis, usa a camada suplementar
embutida em vez de repetir o nível normal. AltGr nativo continua tendo prioridade.

## Configurações gráficas (2026-09-07)

`--settings` abre uma janela Rust/GTK4 para tema, fundo, idioma, disposição,
alinhamento, espaçamentos, escala, relógio/avatar, inatividade e uso de controle.
O layout salvo sobrescreve o do tema. O preview usa alterações ainda não salvas
na mesma janela, sem habilitar captura de controle. Mudanças somente de CSS preservam o vídeo; mudanças estruturais reconstroem o conteúdo sem trocar a janela nativa. Os testes verificam
persistência, preservação do serviço PAM, rejeição de tema inválido, preview sem
salvar e descarte dos controles ao fechar. Capturas reais em inglês estão nos
READMEs em inglês e português.

Ícones de energia: área comum para os três botões e glifo de desligar ampliado.
No preview, hover e tooltips continuam funcionando; nenhum callback de systemctl
é conectado. As dicas incluem o nome da ação e indicam que ela não será executada.
