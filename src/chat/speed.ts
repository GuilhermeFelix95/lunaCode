/// Velocidade medida: tokens de saída do turno ÷ tempo desde o primeiro texto da resposta.
/// Sem tokens medidos, não há taxa: zero tokens não é uma velocidade, é ausência de dado.
export function tokensPerSecond(tokens: number, startedAt: number | null, now: number): number | null {
  if (tokens <= 0 || startedAt === null || now <= startedAt) return null;
  return Math.round(tokens / ((now - startedAt) / 1000));
}
