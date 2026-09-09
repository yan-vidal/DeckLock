# Fundos animados procedurais — investigação

Investigação de viabilidade, 2026-09-08. **Nada foi implementado.** As sondas
usadas para medir estão em `docs/probes/` (fora do build; para reexecutar,
copie para `examples/` e rode com `scripts/cargo-local run --release --example`).

## Pergunta

Que formato dá animação agradável e clássica para as telas de bloqueio e
repouso, fácil de criar, leve de reproduzir, e com acervo da comunidade —
dentro do que GTK4/Wayland realmente suporta hoje.

## Contexto da base

- GTK 4.22.4 no sistema; crate `gtk4` 0.11 com feature `v4_12`.
- O fundo é um `GtkStack` com um `GtkPicture` que recebe um arquivo de imagem
  ou o paintable do GStreamer (`src/ui.rs:125`, `set_background`).
- Vídeo já funciona (`src/media.rs`), com mídia própria em `assets/media`.
- Pré-renderizado em 3D é vídeo: mesmo decoder, mesmo peso, resolução fixa.
  O que muda o jogo é gerar em tempo de execução.

## Medições

Ambiente: contêiner Arch, GL por **llvmpipe** (rasterizador de software, Mesa
26.2.1). São números de piso, não de teto — numa GPU real o lado GSK melhora
muito; o custo de CPU do Cairo permanece.

Cairo puro, offscreen, 1920×1080 ARgb32 (`docs/probes/cairo-cost.rs`):

| desenho | ms/quadro | fração de um quadro a 60 Hz |
|---|---|---|
| gradiente de tela cheia | 15,94 | 96% |
| gradiente + 200 partículas | 15,62 | 94% |
| gradiente + 800 partículas | 19,22 | 115% |
| gradiente + partículas + curva | 18,00 | 108% |
| só a curva (600 segmentos) | 1,96 | 12% |

Janela GTK real, FPS medidos por tick callback (`docs/probes/gtk-fps.rs`),
todos com `GskGLRenderer`:

| modo | conteúdo | fps |
|---|---|---|
| small | camada vetorial em 480×270 | 58,8 |
| static | camada vetorial em 1920×1080 | 38,8 |
| fullscreen | só gradiente em 1920×1080 | 28,0 |
| overlay | gradiente + vetorial em 1920×1080 | 26,5 |

**Leitura:** o custo escala com a *área redesenhada por quadro*, não com a
complexidade do desenho. 200 partículas e uma curva custam ~2–4 ms; preencher
a tela inteira custa ~16 ms. O mesmo conteúdo em 480×270 dobrou o FPS. Com
`GtkDrawingArea` em tela cheia, cada quadro produz e envia uma textura de
1920×1080 (~8 MB) — é isso que domina.

## Formatos avaliados

| formato | procedural | dependência nova | acervo da comunidade | veredito |
|---|---|---|---|---|
| vídeo | não | nenhuma (já existe) | qualquer banco de vídeo | já suportado; pesado, resolução fixa |
| imagem animada (WebP/APNG/GIF) | não | decodificação por código próprio | grande | mesma classe do vídeo, sem ganho |
| Lottie | não (keyframes) | **fora dos repos do Arch** | muito grande (LottieFiles) | ver ressalva |
| shader GLSL | sim | contexto GL no processo | o maior (Shadertoy) | ver ressalva |
| Cairo por quadro | sim | nenhuma | nenhum reaproveitável | viável com limite de área |
| cena GSK (nós/widgets) | sim | nenhuma | nenhum reaproveitável | caminho barato no GTK |

### Lottie

Bindings Rust ativos (`rlottie`, `tlottie`, `dotlottie-rs`) e widget GTK
(`gtk-rlottie-rs`). Mas **nem `rlottie` nem `thorvg` estão nos repositórios
oficiais do Arch** — só `qt6-lottie`, que é Qt e não serve. Adotar Lottie
significa AUR ou vendorizar, o que quebra a disciplina do `PKGBUILD`, que hoje
declara apenas dependências dos repos oficiais. Além disso Lottie é animação
vetorial com keyframes autorada no After Effects: é "vídeo vetorial", não
procedural.

### Shader GLSL

`GskGLShader` foi **depreciado no GTK 4.16** porque o novo pipeline de
renderização focado em Vulkan (introduzido no 4.14) não o suporta. O caminho
suportado é `GtkGLArea` com GL cru. Para um bloqueador de tela isso significa
criar contexto GL e compilar shader de terceiros dentro do processo que guarda
a senha, com fallback para software quando o GL falhar. Risco alto para o
benefício.

### XScreenSaver

`extra/xscreensaver 6.15` está empacotado e tem ~250 *hacks* que são
exatamente "agradáveis e clássicos". Mas são binários X11 separados. Embutir
num bloqueador Wayland exigiria XWayland e um processo externo desenhando
sobre a tela de bloqueio — o oposto do isolamento que o projeto construiu
(o gate `wayland-protocol` existe justamente para garantir esse isolamento).
Serve como **fonte de ideias e algoritmos**, não como formato de importação.
O código é GPL; reimplementar a ideia é diferente de copiar o código, e a
licença precisa ser respeitada se algum trecho for portado.

## Recomendação

Fundo estático (imagem, que já funciona) com uma camada procedural vetorial
por cima, desenhada numa superfície menor e escalada — não repintar 1080p por
quadro. Efeitos clássicos que cabem nesse orçamento: deriva de partículas,
campo de estrelas, curvas de Lissajous, chuva, ondas, relógio analógico.

Formato: descrição declarativa no tema, no mesmo TOML do resto, nomeando um
efeito embutido e seus parâmetros — não um formato de arquivo novo, não um
interpretador. Algo como:

```toml
[background.animation]
effect = "starfield"
density = 120
speed = 0.3
```

Vantagens: nenhuma dependência nova, nenhum decoder, tema em texto em vez de
mídia, independente de resolução, e o custo fica sob controle porque a área
de desenho é escolhida por nós, não pela tela.

## Em aberto

- Medir numa GPU real (o alvo tem uma; aqui só havia llvmpipe).
- Custo de energia em bateria — um bloqueador anima por muito tempo; talvez
  precise parar a animação no modo de repouso ou reduzir a taxa.
- Definir o conjunto inicial de efeitos e se o usuário pode combiná-los.

## Fontes

- [Gsk.GLShader (docs.gtk.org)](https://docs.gtk.org/gsk4/class.GLShader.html)
- [New renderers for GTK (GTK Development Blog)](https://blog.gtk.org/2024/01/28/new-renderers-for-gtk/)
- [GLShader in gsk4 (gtk-rs)](https://gtk-rs.org/gtk4-rs/git/docs/gsk4/struct.GLShader.html)
- [rlottie (crates.io)](https://crates.io/crates/rlottie)
- [gtk-rlottie-rs](https://github.com/paper-plane-developers/gtk-rlottie-rs)
- [Lottie implementations](https://lottie.github.io/implementations/)


## Revisão Codex — escopo proposto para 0.2

Revisão de código e fontes em 2026-09-08; as medições acima não foram
reexecutadas nesta revisão. O usuário reservou esta funcionalidade para a 0.2,
junto de outras mudanças ainda por definir. Nenhuma implementação iniciada.

A direção de efeitos internos em Rust e presets declarativos é adequada, mas
as sondas ainda não sustentam todas as conclusões de desempenho:

- `small` desenha numa área de 480×270 no canto da janela; não amplia essa
  superfície para 1080p. Portanto não mede a composição final proposta nem a
  qualidade visual após ampliação. Raios e espessura também não são escalados.
- A sonda conta callbacks de atualização, não apresentações confirmadas. Divide
  por quatro segundos fixos, embora o intervalo real possa ultrapassá-los.
  Usar tempo efetivo, dimensões alocadas e tempos de apresentação disponíveis.
- 58,8 versus 38,8 equivale a cerca de 1,52×, não ao dobro. O ensaio sugere custo
  associado à área, mas não isola upload, rasterização e composição. Os 8 MB são
  o tamanho de um buffer ARGB32, não uma transferência medida por quadro.
- llvmpipe descreve um ambiente específico; não estabelece um limite inferior
  garantido de desempenho em qualquer GPU. Faltam medições no hardware alvo,
  consumo de energia e comparação com o vídeo já suportado.
- A superfície Cairo offscreen é reutilizada sem limpeza no teste só de curva;
  isso não representa sozinho uma camada animada transparente completa.

A depreciação de GskGLShader está confirmada pela documentação oficial. GtkGLArea
continua suportado; isso não torna todo shader inerentemente inseguro. Para a
primeira versão, efeitos internos revisados reduzem a complexidade. Importar
código arbitrário de terceiros seria uma decisão separada.

Lottie não exige After Effects: a lista oficial também inclui Glaxnimate e
outros editores. Vendorizar uma biblioteca não é incompatível por si só com CI
ou PKGBUILD; exige manutenção, licenças e builds verificados. Não assumir uma
única licença para todo XScreenSaver sem verificar os arquivos concretos.

Proposta para avaliar: campo de estrelas, partículas suaves e curvas de
Lissajous. Comparar nós GSK nativos com Cairo em superfície menor realmente
ampliada, usando o mesmo conteúdo. Presets externos TOML escolheriam efeitos
internos, cores, velocidade, densidade e limite de quadros; a sintaxe acima
continua ilustrativa e não é suportada pelo binário atual. Planejar preview,
paridade CLI/GUI, seed/tempo injetados nos testes e descarte de timers ao ocultar
ou trocar o fundo. Não prometer menor consumo que vídeo antes de medir.

Fontes adicionais de revisão:
- https://docs.gtk.org/gtk4/method.Widget.add_tick_callback.html
- https://lottie.github.io/implementations/


## Implementação 0.2 autorizada

Após a revisão, o usuário autorizou implementar agora os três efeitos. O primeiro
caminho usa textura Cairo transparente limitada a 640 pixels no maior lado,
realmente ampliada por GtkPicture. As tabelas externas efetivas são `[animation]`
e `[idle_animation]` em config.toml (ver config.example.toml); a sintaxe de tema
esboçada anteriormente não foi adotada. Não há importação de código nem novos
decoders. O preview e a interface traduzida permitem ajustar ambos os modos.
Os resultados das sondas históricas não são benchmarks desta implementação.


## Revisão do fluxo solicitada pelo usuário — mídias, não camadas

O usuário esclareceu que cada procedural deve aparecer na biblioteca, com olho e
engrenagem, ser adicionado ao pool e substituir a mídia normal. A implementação
anterior de overlay foi substituída. Os três IDs internos são procedural:starfield,
procedural:particles e procedural:lissajous. Parâmetros por item ficam em
procedurals.<id> no config.toml, compartilhados entre pools. O olho reutiliza o
visualizador de mídia; a engrenagem altera um rascunho validado, persistido pelo
Salvar principal. Cores incluem fundo opaco. Imagens continuam em slideshow;
vídeos e procedurais selecionados permanecem durante a sessão. O formato não
executa código externo. A migração lê rascunhos da versão de overlay não publicada.
