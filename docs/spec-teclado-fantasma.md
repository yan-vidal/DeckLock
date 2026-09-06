> Documento histórico da implementação Python (`7459bb1`). Para o Rust atual,
> consulte [o estado da migração](rust-migration.md) e o README.

# Teclado fantasma para Steam Deck

Data: 2026-09-01

## Problema

O teclado na tela do sc-controller (`scc osd-keyboard`) funciona no Hyprland, mas é opaco e
ocupa 812x417 px no centro da tela, obstruindo o conteúdo enquanto se digita. O teclado
equivalente do SteamOS não pode ser reaproveitado: ele faz parte do cliente Steam, que é
proprietário.

## Objetivo

Um teclado invisível em repouso, que revela apenas as teclas ao redor do dedo, com o
gradiente desvanecendo nas bordas do raio. Ocupa a largura inteira do monitor em foco e uma
fração configurável da altura.

## Restrições

- Não modificar nada em `/usr/lib` — o pacote `sc-controller` (extra, 0.7.1) deve permanecer
  intocado para sobreviver a `pacman -Syu`.
- Ambiente: Arch, Hyprland 0.56.2, Wayland, dois monitores (eDP-1 800x1280, DP-1 1920x1080).
- Não há pressão analógica nos pads do Deck: a struct `DeckInput` não expõe esse campo. A
  seleção é o clique físico do pad (`LPAD`/`RPAD` → `OSK.press`), que já é o comportamento atual.

## Abordagem

Subclasse em Python das duas classes do sc-controller, num executável próprio.

    GhostKeyboardImage(KeyboardImage)   on_draw com alpha por distância
    GhostKeyboard(Keyboard)             _create_background, on_event,
                                        set_cursor_position, show

Tudo o mais é herdado: os dois cursores, a seleção por clique, o layout das teclas, os
modificadores, a captura do controle e a comunicação com o daemon.

## Mecânica

Detecção de toque exata via `mapper.buttons & (LPADTOUCH | RPADTOUCH)` dentro de `on_event` —
sem heurística de timeout. A posição de cada cursor em pixels já é mantida pelo código
original em `cursor.position`, e cada tecla expõe `x, y, w, h`.

Para cada tecla, com `d` = distância do centro da tecla ao cursor:

    alpha = max sobre os lados que estão tocando de (1 - smoothstep(0, raio, d))

O `max` faz uma tecla no meio do teclado responder ao cursor mais próximo, de modo que
tocar os dois pads acende as duas regiões sem uma divisa dura entre elas.

## Decisões

**Escala dos eixos é independente.** O SVG é 800x405. Escalar proporcionalmente para 1920 de
largura levaria a altura a 972 px de 1080, cobrindo a tela — o oposto do objetivo. Portanto:
largura = 100% do monitor, altura = fração configurável (padrão 0.35). As teclas ficam mais
largas que altas, o que é irrelevante num teclado invisível e faz o movimento horizontal do
dedo cobrir mais tela, que é o comportamento desejado.

**Redraw coalescido.** O código original só redesenha ao trocar de tecla; o gradiente exige
redesenhar a cada movimento, e o pad reporta a ~87 Hz. Os redraws são agrupados num timer de
~16 ms usando o `TimerManager` que a classe já possui.

**Monitor em foco via hyprctl.** `get_active_screen_geometry()` do sc-controller depende de
`Gdk.Screen.get_active_window()`, que retorna `None` em Wayland. O monitor focado é obtido de
`hyprctl monitors -j` e fixado com `GtkLayerShell.set_monitor`.

**Repouso com indicador mínimo.** Sem nenhum toque, desenha apenas um traço fino na borda
inferior, confirmando que o teclado está ativo e capturando o controle. `alpha_repouso: 0`
desliga isso e dá invisibilidade absoluta.

## Configuração

`~/.config/scc/ghost-osk.json`:

    raio            180     px de revelação ao redor do dedo
    curva           smoothstep | linear
    altura_tela     0.35    fração da altura do monitor
    alpha_repouso   0.15    indicador em repouso (0 = invisível)
    fps             60      teto de redraw

## Entrega em duas etapas

1. Gradiente e indicador de repouso, no tamanho nativo (800x405). Na tela do Deck isso já é
   largura total, porque 800 é exatamente o viewBox do SVG.
2. Escala para a largura do monitor em foco. Exige reescrever `set_cursor_position`, porque o
   original mistura coordenadas do SVG (`limit`, `button.contains`) com pixels da janela
   (`get_allocation()`) — o que só coincide no tamanho nativo.

## Verificação

Sem framework de teste. Flag `--debug-alpha` força todos os alphas a 1.0, separando erros de
geometria/escala de erros do gradiente. Critério de aceite: digitar uma frase completa na tela
do Deck e outra no monitor externo.

## Risco

A subclasse depende de internas do sc-controller: `KeyboardImage.on_draw`, `cursor.position`,
`mapper.buttons`, `Button.contains`. Um update do pacote pode quebrá-la. Mitigação: o patch é
pequeno, fica versionado em git, e a versão do pacote que funciona está registrada aqui (0.7.1).
