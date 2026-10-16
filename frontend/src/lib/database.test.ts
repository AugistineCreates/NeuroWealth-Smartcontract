import { describe, it, expect, vi, beforeEach } from 'vitest';
import { getEarningsSummary, getPortfolioValueHistory, getRecentTransactions } from './database';
import { server } from './stellar';


vi.mock('./supabase', () => ({
  supabase: {
    from: vi.fn(() => ({
      select: vi.fn().mockReturnThis(),
      eq: vi.fn().mockReturnThis(),
      single: vi.fn(),
      order: vi.fn().mockReturnThis(),
      limit: vi.fn().mockReturnThis(),
      gte: vi.fn().mockReturnThis(),
    })),
  },
}));

vi.mock('./stellar', () => ({
  server: {
    getLatestLedger: vi.fn(),
    getEvents: vi.fn(),
  },
}));

describe('database', () => {
  const originalEnv = process.env;

  beforeEach(() => {
    vi.clearAllMocks();
    process.env = { ...originalEnv };
  });

  describe('getEarningsSummary', () => {
    it('returns zeros if no userAddress provided', async () => {
      process.env.NEXT_PUBLIC_SUPABASE_URL = 'http://localhost';
      const result = await getEarningsSummary();
      expect(result).toEqual({ today: 0, week: 0, month: 0 });
    });

    it('returns zeros if supabaseUrl is missing', async () => {
      delete process.env.NEXT_PUBLIC_SUPABASE_URL;
      const result = await getEarningsSummary('address');
      expect(result).toEqual({ today: 0, week: 0, month: 0 });
    });
  });

  describe('getRecentTransactions', () => {
    it('uses fallback RPC when supabaseUrl is missing', async () => {
      delete process.env.NEXT_PUBLIC_SUPABASE_URL;
      process.env.NEXT_PUBLIC_VAULT_CONTRACT_ID = 'contract_id';
      
      vi.mocked(server.getLatestLedger).mockResolvedValue({ sequence: 100000 } as any);
      vi.mocked(server.getEvents).mockResolvedValue({ events: [] } as any);

      const result = await getRecentTransactions('address');
      expect(result).toEqual([]);
      expect(server.getEvents).toHaveBeenCalled();
    });

    it('uses supabase when configured', async () => {
      process.env.NEXT_PUBLIC_SUPABASE_URL = 'http://localhost';
      
      const mockSingle = vi.fn().mockResolvedValue({ data: { id: 'user_id' } });
      const mockEq = vi.fn().mockReturnValue({ single: mockSingle });
      const mockSelect = vi.fn().mockReturnValue({ eq: mockEq });
      
      (supabase.from as any).mockReturnValueOnce({ select: mockSelect });

      // Mock for deposits/withdrawals
      (supabase.from as any).mockReturnValue({
        select: vi.fn().mockReturnValue({
          eq: vi.fn().mockReturnValue({
            order: vi.fn().mockReturnValue({
              limit: vi.fn().mockResolvedValue({ data: [] }),
            })
          })
        })
      });

      const result = await getRecentTransactions('address');
      expect(result).toEqual([]);
      expect(supabase.from).toHaveBeenCalledWith('users');
      expect(server.getEvents).not.toHaveBeenCalled();
    });
  });
});
