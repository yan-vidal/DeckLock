# Temas externos

Copie `themes/default` para um diretório seu e use `--theme /caminho/do/tema`.
Reabra o preview depois de editar. Recarregamento automático ainda não existe.

```toml
name = "Meu tema"
css = "style.css"
# background = "fundo.webm"

[layout]
alignment = "center"        # start, center, end (alinhamento horizontal)
arrangement = "vertical"    # vertical ou horizontal: relógio/formulário
spacing = 16                # 0..128
padding = 32                # 0..256
clock_visible = true
avatar_visible = true
keyboard_scale = 1.0        # 0.5..2.0
```

`background` pode ser um arquivo local ou diretório. A configuração do aplicativo
tem precedência sobre o fundo do tema. Imagens são esticadas para preencher a área, como no Python;
vídeos são mudos e repetidos. Um erro de vídeo mantém o fundo opaco.

O CSS é **GTK4 CSS**, não CSS de navegador. Use cores, fontes, bordas, transparência,
espaçamento e estados dos widgets. Flexbox, CSS Grid e JavaScript não fazem parte
dessa API; a organização dos blocos é definida pelo TOML.

| Seletor | Elemento |
| --- | --- |
| `window.decklock` | Janela e cor de fundo |
| `#background`, `#veil` | Mídia e camada sobre o fundo |
| `#content`, `#credentials` | Disposição geral e formulário |
| `#clock`, `#date` | Relógio e data |
| `#avatar`, `#username` | Avatar e nome do usuário |
| `#password`, `#submit` | Campo de senha e envio |
| `#status`, `#caps` | Mensagens e Caps Lock físico |
| `#status.warning`, `#status.locked` | Aviso de tentativas restantes e de conta bloqueada |
| `#keyboard`, `.key` | Teclado virtual |
| `.key-hover`, `.modifier-active` | Posição do pad e modificadores virtuais |
| `#power`, `#preview-banner` | Energia e indicação de preview |

```css
@define-color accent #d8a657;
#clock { font-size: 88px; font-weight: 300; }
#content { background-color: rgba(20, 24, 30, 0.82); border-radius: 28px; }
#submit { background-color: @accent; color: #14181e; }
```

CSS inválido ou TOML desconhecido é rejeitado antes de adquirir o bloqueio.
`--check-config` verifica TOML e traduções; a análise de CSS exige abrir o preview.
O nome do serviço PAM pertence à configuração do aplicativo, não ao tema.

O teclado fica ancorado embaixo. No modo mouse, abrir o teclado esconde relógio,
avatar e nome para reservar espaço ao campo de senha, como no Python. Com controle,
as teclas aparecem por proximidade dos dedos. Para conferir a composição, use
`--preview-fullscreen`. `keyboard_scale` multiplica a escala original: 0,62 no modo
mouse e 1,0 no modo controle.
Ainda não há layout arbitrário por GtkBuilder, editor visual ou API de plugins.

Para importar as cores locais do Python, execute `scripts/import-python-theme`.
O diretório gerado é um tema CSS/TOML comum; a importação não altera o sc-controller.

## Editor visual

`scripts/cargo-local run -- --settings` abre o editor GTK4. O botão de preview usa
uma cópia temporária das escolhas; salvar grava `~/.config/decklock/config.toml`
(ou o arquivo indicado por `--config`) por substituição atômica.

A seção `[layout]` do arquivo do usuário tem prioridade sobre `[layout]` do tema.
O tema e seu CSS não são editados. Remova a seção do usuário para voltar a herdar
a disposição do tema. Campos não expostos, como serviço PAM e fundo ocioso, são
preservados. Comentários/formatação do TOML são normalizados ao salvar.
