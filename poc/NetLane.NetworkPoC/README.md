# NetLane Network PoC

A descoberta padrão não aplica políticas. O modo --wfp usa o motor nativo de políticas de conexão por AppId/LUID; o resultado de cada regra informa se o Windows aceitou sua instalação. O modo --firewall é legado e recusa selecionar Wi-Fi/Ethernet: bloquear não é redirecionar.

## Teste de direcionamento real

Consulte primeiro [pré-requisitos, ativação temporária e reversão](../../docs/roteamento-nativo.md).

Em PowerShell administrativo, com routepolicies habilitado para IPv4/IPv6:

```powershell
dotnet run --project "C:\Codes\netlane\poc\NetLane.NetworkPoC" --configuration Release -- --verify-routing
```

- Alvo exclusivo: o próprio NetLane.NetworkPoC.exe, sem editar netlane-rules.json.
- Executa processos filhos novos, sem bind manual, proxy do ambiente ou IP_UNICAST_IF.
- Mede controle automático, Ethernet, Wi-Fi e controle após remoção.
- HTTPS em api.ipify.org e consulta DNS UDP a 1.1.1.1:53.
- Compara os endereços locais medidos com a placa escolhida; IP público diferente não é requisito quando dois links compartilham NAT.
- Em seleção ambígua, exige --wifi-interface "GUID" e --ethernet-interface "GUID".
- Exit 0: prova IPv4 passou; 1: falha/incompleta; 2: pré-requisitos ausentes, sem aplicar políticas.
- IPv6 e tráfego do Steam exigem verificação adicional.

--probe-child é a rotina interna de medição sem aplicar regra; usá-la isoladamente testa somente a rota automática atual.

## Consulta antiga de IP público

--check-public-ip executa curl --interface para consultar a saída disponível de cada adaptador. Não mede o aplicativo mapeado e não comprova funcionamento do motor. Os logs históricos da Fase 0 devem ser lidos com esse limite.
