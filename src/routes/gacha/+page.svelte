<script lang="ts">
  import { Sparkles, TrendingUp, BarChart2, FileDown } from 'lucide-svelte';
</script>

<div class="gacha-page">
  <div class="page-header">
    <div class="header-left">
      <h2 class="page-title">寻访分析</h2>
      <p class="page-subtitle">抽卡记录统计与分析</p>
    </div>
    <button class="btn-export" disabled>
      <FileDown size={15} />
      <span>导出记录</span>
    </button>
  </div>

  <div class="placeholder-content">
    <div class="placeholder-icon">
      <Sparkles size={48} strokeWidth={1} />
    </div>
    <h3>寻访分析功能开发中</h3>
    <p>此功能需要通过游戏 API 获取寻访记录，正在调研中</p>

    <div class="feature-list">
      <div class="feature-item">
        <TrendingUp size={16} />
        <span>六星出货率统计</span>
      </div>
      <div class="feature-item">
        <BarChart2 size={16} />
        <span>每期寻访分析</span>
      </div>
      <div class="feature-item">
        <FileDown size={16} />
        <span>导出 Excel / CSV / JSON</span>
      </div>
    </div>
  </div>
</div>

<style>
  .gacha-page {
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 32px;
    gap: 24px;
  }

  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
  }

  .header-left {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .page-title {
    font-size: 22px;
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .page-subtitle {
    font-size: 13px;
    color: var(--color-text-muted);
  }

  .btn-export {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    color: var(--color-text-secondary);
    font-size: 13px;
    font-family: var(--font-ui);
    cursor: not-allowed;
    opacity: 0.5;
  }

  .placeholder-content {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--color-text-muted);
    text-align: center;
  }

  .placeholder-icon {
    color: var(--color-text-muted);
    opacity: 0.4;
    margin-bottom: 8px;
  }

  .placeholder-content h3 {
    font-size: 16px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }

  .placeholder-content p {
    font-size: 13px;
    max-width: 320px;
    line-height: 1.6;
  }

  .feature-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 16px;
    padding: 16px 24px;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    text-align: left;
  }

  .feature-item {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    color: var(--color-text-secondary);
  }
</style>
