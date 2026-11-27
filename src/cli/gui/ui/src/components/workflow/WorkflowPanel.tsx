import { useState } from 'react';
import { ChevronDown, ChevronRight, CheckSquare, Square, Bot, Cpu } from 'lucide-react';
import { useTranslation } from '../../i18n/useTranslation';
import type { WorkflowState, AgentStatus } from '../../services/bridge';
import { cn } from '../../utils/helpers';

interface WorkflowPanelProps {
  workflow: WorkflowState;
}

type Tab = 'todos' | 'plan' | 'agents';

export function WorkflowPanel({ workflow }: WorkflowPanelProps) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>('todos');
  const [collapsed, setCollapsed] = useState(false);

  const doneTodos = workflow.todos.filter(t => t.done).length;
  const totalTodos = workflow.todos.length;

  return (
    <div
      className="shrink-0 flex flex-col"
      style={{
        width: '280px',
        background: 'var(--ac-bg-sidebar)',
        borderLeft: '1px solid var(--ac-border)',
      }}
    >
      {/* Header */}
      <button
        onClick={() => setCollapsed(v => !v)}
        className="flex items-center gap-2 px-3 py-2.5 w-full"
        style={{ borderBottom: '1px solid var(--ac-border)' }}
      >
        {collapsed ? <ChevronRight size={14} /> : <ChevronDown size={14} />}
        <span className="text-xs font-semibold uppercase tracking-wider" style={{ color: 'var(--ac-text-dim)' }}>
          {t('workflowPanel')}
        </span>
        {totalTodos > 0 && (
          <span
            className="ms-auto text-xs px-1.5 py-0.5 rounded-full"
            style={{ background: 'var(--ac-gold-soft)', color: 'var(--ac-gold)' }}
          >
            {doneTodos}/{totalTodos}
          </span>
        )}
      </button>

      {!collapsed && (
        <>
          {/* Tabs */}
          <div
            className="flex px-2 pt-2 gap-0.5"
            style={{ borderBottom: '1px solid var(--ac-border)' }}
          >
            {(['todos', 'plan', 'agents'] as Tab[]).map(tabId => (
              <button
                key={tabId}
                onClick={() => setTab(tabId)}
                className={cn(
                  'px-3 py-1.5 text-xs font-medium rounded-t-lg transition-all',
                  tab === tabId
                    ? 'text-[var(--ac-text)]'
                    : 'text-[var(--ac-text-dim)] hover:text-[var(--ac-text-muted)]',
                )}
                style={tab === tabId ? { background: 'var(--ac-bg-canvas)' } : undefined}
              >
                {t(tabId as 'todos' | 'plan' | 'agents')}
              </button>
            ))}
          </div>

          {/* Content */}
          <div className="flex-1 overflow-y-auto p-3">
            {tab === 'todos' && <TodosTab todos={workflow.todos} />}
            {tab === 'plan' && <PlanTab plan={workflow.plan} />}
            {tab === 'agents' && <AgentsTab agents={workflow.agents} />}
          </div>
        </>
      )}
    </div>
  );
}

function TodosTab({ todos }: { todos: WorkflowState['todos'] }) {
  const { t } = useTranslation();
  if (todos.length === 0) {
    return <EmptyState label={t('noTodos')} />;
  }
  return (
    <div className="space-y-1.5">
      {todos.map(todo => (
        <div
          key={todo.id}
          className="flex items-start gap-2 px-2 py-2 rounded-lg text-xs"
          style={{ background: 'var(--ac-bg-surface)', border: '1px solid var(--ac-border)' }}
        >
          {todo.done ? (
            <CheckSquare size={13} style={{ color: 'var(--ac-success)', marginTop: 1 }} />
          ) : (
            <Square size={13} style={{ color: 'var(--ac-text-dim)', marginTop: 1 }} />
          )}
          <span
            className={todo.done ? 'line-through' : ''}
            style={{ color: todo.done ? 'var(--ac-text-dim)' : 'var(--ac-text)' }}
          >
            {todo.title}
          </span>
        </div>
      ))}
    </div>
  );
}

function PlanTab({ plan }: { plan: string }) {
  const { t } = useTranslation();
  if (!plan) return <EmptyState label={t('noPlan')} />;
  const lines = plan.split('\n').filter(Boolean);
  return (
    <div className="space-y-1.5">
      {lines.map((line, i) => (
        <div
          key={i}
          className="flex items-start gap-2 px-2 py-1.5 rounded-lg text-xs"
          style={{ background: 'var(--ac-bg-surface)', border: '1px solid var(--ac-border)', color: 'var(--ac-text)' }}
        >
          <span
            className="shrink-0 w-5 h-5 rounded-full flex items-center justify-center text-xs font-bold"
            style={{ background: 'var(--ac-gold-soft)', color: 'var(--ac-gold)' }}
          >
            {i + 1}
          </span>
          {line.replace(/^\d+\.\s*/, '')}
        </div>
      ))}
    </div>
  );
}

function AgentsTab({ agents }: { agents: AgentStatus[] }) {
  const { t } = useTranslation();
  const statusColors: Record<AgentStatus['status'], string> = {
    idle: 'var(--ac-text-dim)',
    working: 'var(--ac-warning)',
    done: 'var(--ac-success)',
    error: 'var(--ac-danger)',
  };

  if (agents.length === 0) return <EmptyState label={t('noAgents')} />;

  return (
    <div className="space-y-2">
      {agents.map(agent => (
        <div
          key={agent.id}
          className="flex items-center gap-2.5 px-3 py-2.5 rounded-xl text-xs"
          style={{ background: 'var(--ac-bg-surface)', border: '1px solid var(--ac-border)' }}
        >
          <div
            className="w-7 h-7 rounded-full flex items-center justify-center shrink-0"
            style={{
              background:
                agent.status === 'working'
                  ? 'rgba(245,204,132,0.15)'
                  : 'var(--ac-bg-elevated)',
            }}
          >
            {agent.status === 'working' ? (
              <Cpu size={13} className="spin" style={{ color: 'var(--ac-warning)' }} />
            ) : (
              <Bot size={13} style={{ color: statusColors[agent.status] }} />
            )}
          </div>
          <div className="flex-1 min-w-0">
            <div className="font-semibold truncate" style={{ color: 'var(--ac-text)' }}>
              {agent.name}
            </div>
            {agent.model && (
              <div className="truncate" style={{ color: 'var(--ac-text-dim)' }}>{agent.model}</div>
            )}
          </div>
          <span
            className="px-1.5 py-0.5 rounded-full text-xs font-medium"
            style={{
              color: statusColors[agent.status],
              background: `${statusColors[agent.status]}22`,
            }}
          >
            {t(('agent' + agent.status.charAt(0).toUpperCase() + agent.status.slice(1)) as any)}
          </span>
        </div>
      ))}
    </div>
  );
}

function EmptyState({ label }: { label: string }) {
  return (
    <div className="text-center py-8 text-xs" style={{ color: 'var(--ac-text-dim)' }}>
      {label}
    </div>
  );
}
