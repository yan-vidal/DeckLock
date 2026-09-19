# Roadmap / Próximos passos

Directions, not delivery promises. / Direções, sem promessa de datas.

## 0.2.0 — released 2026-09-09 / publicada em 2026-09-09

- Procedural media, live editors and preview diagnostics / mídias procedurais, editores ao vivo e métricas.
- Consistent ordinary window controls and bilingual offline help / controles consistentes e ajuda offline bilíngue.
- Reviewed changelog, full version names and documented release checks / changelog revisado, versões completas e processo de release documentado.
- PAM notice reliability: bounded helper output and elapsed-time countdowns / confiabilidade dos avisos PAM: saída limitada do auxiliar e contagem por tempo decorrido.

That 0.2.0 position is superseded by Phase B, which is the concrete packaging need it anticipated: each package installs its own `/etc/pam.d/decklock` including that distribution's stack, and `pam_service` defaults to `decklock`. Installing a service of our own is not editing system authentication rules — nothing else reads that file and it is removed with the package. Distribution files are still never edited.
Aquela posição da 0.2.0 fica superada pela Fase B, que é a necessidade concreta de empacotamento que ela previa: cada pacote instala o seu `/etc/pam.d/decklock` incluindo a pilha daquela distribuição, e o `pam_service` passa a ter `decklock` como padrão. Instalar um serviço próprio não é editar regras de autenticação do sistema — nada mais lê esse arquivo e ele sai junto com o pacote. Arquivos da distribuição continuam nunca sendo editados.

## Next direction — platform reach before new features / Próxima direção — alcance de plataforma antes de novas funcionalidades

Broaden the systems DeckLock runs on before adding features. The project is installable in practice only on Arch today, so a new feature is built for an audience that cannot install it, and each feature multiplies the per-platform validation cost of everything that follows. The disposable QEMU gate with real Sway and real PAM, built for 0.2.0, is the reusable asset that makes this cheap. Discussed in issues #7-#19; no version or date is assigned.
Ampliar os sistemas onde o DeckLock roda antes de acrescentar funcionalidades. Hoje o projeto só é instalável na prática no Arch, então uma funcionalidade nova é construída para um público que não consegue instalá-la, e cada funcionalidade multiplica o custo de validação por plataforma de tudo que vem depois. O gate descartável em QEMU com Sway e PAM reais, feito para a 0.2.0, é o ativo reaproveitável que torna isso barato. Discutido nas issues #7-#19; sem versão ou data atribuída.

- Phase A — more Wayland compositors (#9): verification only, same binary and same package / mais compositores Wayland: só verificação, mesmo binário e mesmo pacote.

  **GNOME and KDE Plasma are not supported** (#7, #8 closed). Both draw their own lock screen inside the desktop and do not let another program replace it: GNOME in `gnome-shell` itself, KDE in `kscreenlocker_greet`, which KWin launches from a fixed path and hands a pre-connected socket. There is no protocol for DeckLock to plug into, so this is not a missing backend. The only routes would be separate implementations inside each desktop — a Plasma lock-screen theme in QML or a GNOME Shell extension in JavaScript — a second product in another language, which is not planned. The X11 backend (Phase C) does not reach them either: GNOME removed its X11 session and Plasma 6.8 drops X11.
  **GNOME e KDE Plasma não são suportados** (#7, #8 fechadas). Os dois desenham a própria tela de bloqueio dentro do desktop e não deixam outro programa substituí-la: o GNOME dentro do próprio `gnome-shell`, o KDE no `kscreenlocker_greet`, que o KWin lança de um caminho fixo e a quem entrega um socket já conectado. Não há protocolo onde o DeckLock possa se encaixar, então não é um backend faltando. Os únicos caminhos seriam implementações separadas dentro de cada desktop — um tema de tela de bloqueio do Plasma em QML ou uma extensão do GNOME Shell em JavaScript —, um segundo produto em outra linguagem, que não está planejado. O backend X11 (Fase C) também não os alcança: o GNOME removeu a sessão X11 e o Plasma 6.8 abandona o X11.
- Phase B — Fedora and Ubuntu packages with their own recorded dependency baselines, per-distribution `pam_service` and lockout notices, and the disposable VM gate per distribution (#10-#14, #24, #33) / pacotes Fedora e Ubuntu com baseline de dependências próprio, `pam_service` e avisos de bloqueio por distribuição, e o gate de VM por distribuição. Debian 13 remains blocked on third-party packaging (#25).
- Phase C — X11 lock backend completed (#15-#18 closed, PRs #30, #32, #33); implemented behind the optional `x11` feature, off by default and not built into the published packages, with reduced-guarantee notice, accelerated video and guide documentation / backend de bloqueio X11 concluído (#15-#18 fechadas, PRs #30, #32, #33); implementado atrás da feature opcional `x11`, desligada por padrão nos pacotes publicados, com aviso de garantia reduzida, vídeo acelerado e documentação nos guias.
- Phase D — other Unix systems (#19 closed, not planned); out of scope to preserve focus on the Linux desktop session / outros sistemas Unix (#19 fechada, não planejado); fora de escopo para manter o foco na sessão desktop Linux.

Widgets/plugins, remote media and the greeter stay behind this reach work.
Widgets/plugins, mídias remotas e greeter ficam atrás desse trabalho de alcance.

## Candidates for later / Candidatos para depois

- Real-device performance and battery measurements / medições de desempenho e bateria no dispositivo.
- More compositor/controller recovery and accessibility coverage / ampliar validação de recuperação e acessibilidade.
- More original media and theme customization / novas mídias autorais e personalização de temas.
- A documentation website built from docs/guide / site de documentação a partir de docs/guide.
- Start with built-in Rust widgets (for example clock and battery), then allow plugins to add new widget types. Lua is the proposed extension language, subject to isolation design and validation; built-in widgets need not depend on Lua. / Começar com widgets nativos em Rust (por exemplo relógio e bateria), depois permitir novos tipos por plugins. Lua é a linguagem proposta para extensões, sujeita ao projeto e à validação do isolamento; widgets nativos não precisam depender de Lua.
- Plugin design must restrict capabilities, CPU and memory, isolate execution, and protect authentication from overlays and input capture. Plugins must never receive credentials or authorize unlock. / O projeto de plugins deve restringir permissões, CPU e memória, isolar a execução e proteger a autenticação de sobreposições e captura de entrada. Plugins nunca recebem credenciais nem autorizam desbloqueio.
- Optional remote media provider with local cache and offline fallback / provedor opcional de mídias remotas com cache local e alternativa offline.
- Investigate a greetd greeter sharing themes and media while keeping login/session management separate / investigar greeter para greetd compartilhando temas e mídias, com gerenciamento de login e sessão separado.

Widgets/plugins, remote media and the greeter are outside 0.2.0; no later version or delivery date is assigned.
Widgets/plugins, mídias remotas e greeter ficam fora da 0.2.0; sem versão posterior ou data de entrega definida.

Use issues to discuss concrete work before assigning a version. X11 locking is accepted for a later version behind an explicit compatibility layer, with the reduced guarantee shown in the UI at lock time and not only in the documentation: an X11 session cannot isolate keystrokes, so another application in the session can read what is typed. Wayland stays the recommended path and the only one with the full guarantee.
Use issues para discutir tarefas concretas antes de atribuir uma versão. O bloqueio X11 está aceito para uma versão futura atrás de uma camada de compatibilidade explícita, com a garantia reduzida exibida na interface no momento do bloqueio e não apenas na documentação: uma sessão X11 não isola as teclas digitadas, então outro aplicativo da sessão pode ler o que é digitado. O Wayland segue sendo o caminho recomendado e o único com a garantia completa.
