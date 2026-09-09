# Por dentro do DeckLock

Este capítulo apresenta a implementação para pessoas curiosas e colaboradores. Descreve o projeto atual, sem garantir segurança ou desempenho em todos os ambientes.

## Rust e GTK

Rust controla configuração, estado da aplicação, lógica do teclado e coordenação da renderização. GTK4 constrói as janelas de configurações/preview e os widgets da tela de bloqueio. CSS estiliza widgets GTK; TOML descreve layout e configuração validados. Temas não executam scripts. Lua/plugins ainda não foram implementados.

O bloqueador real usa gtk4-session-lock e o protocolo ext-session-lock-v1 do compositor. Decorações comuns pertencem apenas às configurações, editores, ajuda e preview. Não há bloqueio para X11.

## Limite da autenticação

A máquina de estados da sessão autoriza desbloquear somente após obter a posse do bloqueio no compositor e receber uma resposta bem-sucedida para a tentativa atual de autenticação. PAM roda por um processo auxiliar separado. Respostas antigas, falhas, preview, sinais e fechamento de janelas não podem autorizar desbloqueio.

A interface não é a autoridade sobre a posse da sessão. Um teste de preview aprovado não demonstra as políticas reais de PAM nem a recuperação do compositor. SIGTERM não é um atalho para desbloquear.

O processo auxiliar devolve aceitação, recusa ou erro pelo código de saída, e agora também o texto que os módulos pedem para exibir: apenas `PAM_ERROR_MSG` e `PAM_TEXT_INFO`, nunca o prompt que respondemos nem a senha. Esse texto é achatado numa linha, limitado em bytes e exibido como rótulo simples, sem interpretar marcação. O auxiliar roda com `LC_ALL=C` para que a redação dos módulos possa ser reconhecida e reescrita no idioma da interface em vez de ser comparada com traduções.

O aviso de bloqueio por falhas vem daí. `pam_faillock` informa "The account is locked due to N failed logins." e, quando aplicável, "(N minutes left to unlock)"; o contador da tela é essa estimativa em minutos, decrementada por tique inteiro. Como reforço opcional, `/etc/security/faillock.conf` e a saída do `faillock(8)` são lidos em melhor esforço para informar quantas tentativas restam — só quando ambos, limite e contagem, forem conhecidos. Ausência, formato inesperado ou ferramenta indisponível resultam em silêncio, nunca em um número estimado: o formato do arquivo de contagem é detalhe interno do módulo.

Esse caminho é só apresentação. Ele não autoriza, não recusa e não atrasa tentativa alguma; o campo de senha permanece habilitado durante a contagem, porque a política é do PAM e só ele decide quando uma tentativa é aceita.

## Fluxo de mídias

GStreamer decodifica vídeo para um paintable do GTK. Imagens e texturas procedurais ocupam a mesma pilha de fundos; fotos transitam com seu crossfade. Seleção e renderização são separadas: vídeo ou procedural permanecem escolhidos naquele bloqueio; fotos usam uma apresentação somente de imagens.

Referências procedurais usam identificadores procedural: em vez de caminhos de arquivos. Parâmetros por item ficam na seção procedurals do TOML e são compartilhados pelos dois pools. Semente e tempo fornecido tornam a geração de quadros reproduzível.

Cairo desenha quadros procedurais em buffers limitados, com a maior dimensão de até 640 pixels. GTK amplia a textura para a janela. Um callback do relógio de quadros limita as atualizações à taxa configurada, de até 30 FPS, e deixa de receber ticks quando o widget está desmapeado. É renderização raster limitada, sem importação de GLSL ou execução de scripts.

Miniaturas de vídeo são decodificadas em sequência por um trabalhador em segundo plano. O catálogo compartilha pedidos pendentes e guarda até 128 resultados concluídos, incluindo falhas, por caminho e metadados do arquivo. Pixels retornam à thread GTK para criar a textura; referências fracas evitam reter linhas descartadas.

## Como interpretar as métricas

CPU de desenho usa o relógio de CPU da thread durante a renderização. CPU do processo usa o relógio do processo; 100 por cento equivale a um núcleo completamente ocupado. RSS vem de /proc/self/status. Esses totais incluem GTK, mídias e tarefas das configurações. FPS gerados não medem latência de apresentação do compositor nem utilização da GPU.

## Configuração e tradução

Serde lê TOML com valores padrão e rejeita campos desconhecidos. A validação precede a gravação atômica. CLI e interface compartilham o esquema; editar um campo não relacionado deve preservar os valores existentes. window_decorations afeta somente janelas comuns.

Catálogos Fluent fornecem inglês e português brasileiro. O guia usa fontes Markdown embutidas durante a compilação e também incluídas no pacote. O leitor offline suporta títulos, parágrafos, listas e blocos de código; não executa HTML, scripts nem conteúdo remoto. Um futuro site estático poderá consumir os mesmos arquivos Markdown.

sc-controller permanece uma dependência externa opcional, comunicando por socket Unix. Pedidos de energia são chamadas ao systemctl; a política do sistema fica fora do DeckLock.

## Trabalhando no projeto

```sh
scripts/cargo-local run -- --settings
scripts/check
scripts/check --all
```

Leia AGENTS.md e docs/testing.md antes de alterar comportamentos. Acrescente uma regressão que falhe no limite afetado: executável CLI, GTK isolado ou protocolo. Use sementes fixas, HOME/XDG privados, daemons fictícios e esperas limitadas. Nunca direcione testes automáticos à sessão, senha PAM ou controle real do usuário.

A suíte completa verifica formatação, Clippy, testes Rust/CLI, compilação dos exemplos, contratos de pacote, whitespace, GTK isolado e um compositor Wayland fictício. Ela não certifica ergonomia, bateria, integração PAM real ou segurança.

## Releases e contribuições

Mudanças passam por PR com verificações obrigatórias. A CI gera artefatos candidatos para Arch. Uma tag de versão sobre código revisado da main repete a verificação, valida a versão e publica artefatos com checksums, changelog revisado e referências automáticas aos PRs.

Cargo e tags usam a versão completa de três partes. pkgrel do Arch acompanha revisões de empacotamento separadamente. A 0.2.0 está em preparação; publicar continua sendo uma etapa separada. O roadmap indica direções, sem prometer datas de entrega.
