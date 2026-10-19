# Tello Pilot

Cliente desktop para o drone **DJI/Ryze Tello**, com os protocolos implementados do zero, sem bibliotecas de Tello.

![Tela de voo](docs/voo.png)

- Vídeo ao vivo (H.264 decodificado pela placa de vídeo via WebCodecs)
- HUD com horizonte artificial, altitude, velocidade, rumo, bateria e sinal
- Controle por **mouse** (sticks virtuais), **teclado** e **controle Xbox** (com vibração)
- Dois modos de conexão:
  - **SDK oficial**: documentado e estável. Detecta Tello, Tello EDU e RoboMaster TT.
  - **Recursos do app**: Throw & Go, pouso na palma, flips 8D, EZ Shots (360°, círculo, Up & Away), modo Bounce, modo rápido e fotos de 5 MP.
- Gravação de vídeo em MP4 e capturas da tela
- Segurança: decolagem segurando o botão, modo iniciante, altura máxima, pouso automático com bateria baixa, emergência sempre visível
- Caixa-preta: log de todos os pacotes, exportável para diagnóstico

## Download

Os executáveis do Windows são gerados pelo GitHub Actions a cada push. Em **Actions → CI → execução mais recente → Artifacts**:

| Artifact | Conteúdo |
|---|---|
| `tello-pilot-windows` | Instalador do app (`Tello Pilot_x.y.z_x64-setup.exe`) |
| `tello-cli-windows` | `tello-cli.exe` (diagnóstico) e `tello-sim.exe` (simulador) |

> O instalador não tem assinatura digital, então o Windows SmartScreen pode avisar. Clique em **Mais informações → Executar assim mesmo**.
>
> O instalador pede permissão de administrador para criar a regra do Firewall. Sem ela, o Windows bloqueia a telemetria e o vídeo, porque classifica a rede do Tello como "Pública".

## Primeiro teste: diagnóstico sem decolar

Antes de voar, rode o `tello-cli.exe` com o drone ligado e o computador no Wi-Fi `TELLO-XXXXXX`:

```
tello-cli.exe
```

Ele **não decola**. O programa:

1. confere se o computador está na rede do drone;
2. conecta e mostra modelo, versão do SDK, serial e bateria;
3. mostra a telemetria por 10 s;
4. grava 10 s de vídeo em `tello-video-AAAAMMDD-HHMMSS.mp4`;
5. salva o log `tello-log-AAAAMMDD-HHMMSS.jsonl`.

No final aparece um resumo `[ok]` / `[falha]` para cada item. Se algo falhar, o arquivo de log mostra exatamente o que foi enviado e recebido.

Opções: `--ip`, `--segundos`, `--saida <pasta>`. O modo `tello-cli.exe shell` abre um console para digitar comandos do SDK (`battery?`, `takeoff`, `land`…).

## Usando o app

![Tela de conexão](docs/conectar.png)

1. Ligue o drone e conecte o computador ao Wi-Fi dele.
2. Escolha o modo (**SDK oficial** ou **Recursos do app**) e clique em **Conectar**.
3. O vídeo liga sozinho. Para decolar, **segure** o botão "Segure para decolar" por 1 segundo.

### Controles

| Ação | Mouse | Teclado | Controle Xbox |
|---|---|---|---|
| Subir/descer · girar | Stick esquerdo | W/S · A/D | Stick esquerdo |
| Frente/trás · lados | Stick direito | Setas | Stick direito |
| Decolar | Segurar o botão | Segurar T | Segurar Menu |
| Pousar | Botão Pousar | L | Menu (no ar) |
| Pairar | Botão Pairar | Espaço | B |
| Foto | Botão Foto | P | A |
| Gravar vídeo | Botão Gravar | R | X |
| Flips | Painel 8D | — | LB + D-pad (duas direções = diagonal) |
| Throw & Go / pouso na palma | Botões | — | RB |
| Modo rápido (modo App) | Chave | — | Y |
| **Emergência** (corta os motores) | Botão vermelho | Esc duas vezes | View + Menu |

O mapeamento segue o **modo 2** (o do app oficial). Nas configurações dá para trocar para o modo 1, ajustar zona morta e expo, e inverter o eixo frente/trás.

### Segurança

- **Modo iniciante** (ligado por padrão): sticks limitados a 40% e altura máxima de 3 m.
- **Pouso automático** quando a bateria chega a 10% (configurável), com alertas em 20% e 15% e vibração no controle.
- Se o controle desconectar, a janela perder o foco ou o app parar de mandar comandos, os sticks voltam ao centro e o drone **paira**.
- Fechar o app com o drone voando abre uma confirmação para pousar antes.
- Flips ficam desativados abaixo de 50% de bateria, porque o próprio drone recusa.

### Onde ficam os arquivos

| Tipo | Pasta |
|---|---|
| Fotos de 5 MP e capturas | `Imagens\Tello Pilot` |
| Vídeos gravados (MP4) | `Vídeos\Tello Pilot` |
| Logs exportados | `Documentos\Tello Pilot\logs` |

## Problemas comuns

| Sintoma | Causa provável |
|---|---|
| "O drone não respondeu" | O computador não está no Wi-Fi do drone, o drone está desligado ou o app oficial está aberto no celular. |
| Conecta, mas sem telemetria/vídeo | Firewall do Windows bloqueando a rede pública. Reinstale como administrador ou libere o Tello Pilot em **Segurança do Windows → Firewall → Permitir um aplicativo** (marque *Redes públicas*). |
| Recurso aparece desativado | Ele não existe no modo ou modelo atual. Passe o mouse sobre o botão para ver o motivo. |
| Modo App não conecta | Desligue e ligue o drone: depois de usar o SDK ele pode continuar nesse modo até reiniciar. |

Em qualquer problema, abra a **caixa-preta** (ícone de inseto) e use **Exportar log**.

## Desenvolvimento

Requisitos: Rust (stable), Node.js 22. No Linux, também as dependências do Tauri (`libwebkit2gtk-4.1-dev` e afins).

```bash
npm install

cargo test -p tello-core -p tello-sim -p tello-cli   # testes (unitários + integração com o simulador)
npm run check                                         # checagem de tipos da interface

npm run dev          # só a interface, no navegador, com um backend simulado
npx tauri dev        # app completo
cargo run -p tello-sim                                # simulador de Tello em 127.0.0.1
cargo run -p tello-cli -- --ip 127.0.0.1              # diagnóstico contra o simulador
```

Com o simulador rodando, use **Usar simulador** na tela de conexão (IP `127.0.0.1`). Ele funciona nos dois modos (SDK e App) e envia vídeo de teste, telemetria e fotos.

### Estrutura

```
crates/tello-core   protocolos (SDK e App), vídeo, gravação MP4, segurança, caixa-preta
crates/tello-sim    simulador do drone + testes de integração
crates/tello-cli    diagnóstico e console de comandos
src-tauri           app desktop: ponte entre o núcleo e a interface
ui                  interface (Svelte 5 + TypeScript)
```

### Dependências

O que é o coração do projeto é feito à mão: os protocolos do Tello, CRCs, montagem do vídeo, parser de SPS e o gravador MP4. O que é infraestrutura grande vem pronto: decodificação de vídeo (WebCodecs, do próprio sistema), controle (Gamepad API), janela e instalador (Tauri), interface (Svelte) e serialização (serde).

### Protocolos

- **SDK oficial**: comandos de texto em UDP para `192.168.10.1:8889`. Estado a ~10 Hz na porta local 8890, vídeo H.264 na 11111. `rc` a 20 Hz mantém o drone no ar (ele pousa sozinho após 15 s sem comandos).
- **Protocolo do app**: pacotes binários `0xCC` com CRC-8/CRC-16, handshake `conn_req:`, sticks a 50 Hz, telemetria de voo (0x56), atitude pela IMU (0x1051) e transferência de fotos (0x62/0x63/0x64). O formato vem de documentação pública da comunidade. Os CRCs já foram validados com um pacote real publicado; os demais detalhes serão confirmados com os logs dos primeiros voos (caixa-preta).

## Próximos passos

- macOS e Linux (mesmo código; no Linux o WebKitGTK pode não ter H.264 no WebCodecs)
- Android (Tauri 2), com suporte ao controle e ao Wi-Fi sem internet do drone

## Licença

GPL-3.0. Ícones do [Lucide](https://lucide.dev) (licença ISC).
