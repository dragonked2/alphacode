import { useState, useMemo } from 'react';
import {
  Users,
  Bot,
  Clock,
  CheckCircle2,
  Skull,
  Send,
  MessageSquare,
  Hash,
  Cpu,
  Loader2,
  Play,
  ListChecks,
  Compass,
  AlertCircle,
} from 'lucide-react';
import { cn, formatTime } from '../../utils/helpers';
import { useTranslation } from '../../i18n/useTranslation';
import type { WorkflowState, AgentStatus, TodoItem } from '../../services/bridge';

export type AgentState = 'working' | 'waiting' | 'done' | 'killed';

export interface SwarmAgent {
  id: string;
  name: string;
  role: string;
  model: string;
  state: AgentState;
  avatarColor: string;
  brief: string;
  tasksCompleted?: number;
  totalTasks?: number;
}

export interface SwarmBoardMessage {
  seq: number;
  agent: string;
  text: string;
  timestamp: number;
  kind?: 'brief' | 'update' | 'handoff' | 'done';
}

interface SwarmVisualizerProps {
  className?: string;
  title?: string;
  workflow?: WorkflowState;
  agents?: SwarmAgent[];
  messages?: SwarmBoardMessage[];
  onPostMessage?: (text: string) => void;
  onLaunchSwarm?: (task: string) => void;
}

const AVATAR_GRADIENTS = [
  'from-amber-400 to-amber-600',
  'from-emerald-400 to-teal-600',
  'from-sky-400 to-blue-600',
  'from-purple-400 to-indigo-600',
  'from-rose-400 to-red-600',
];

function mapAgentStatusToSwarmAgent(a: AgentStatus, idx: number): SwarmAgent {
  const mappedState: AgentState =
    a.status === 'working'
      ? 'working'
      : a.status === 'done'
        ? 'done'
        : a.status === 'error'
          ? 'killed'
          : 'waiting';
  return {
    id: a.id || `agent-${idx}`,
    name: a.name || a.id || `agent-${idx}`,
    role: a.role || (a.model ? a.model : 'Swarm Agent'),
    model: a.model || '',
    state: mappedState,
    avatarColor: AVATAR_GRADIENTS[idx % AVATAR_GRADIENTS.length],
    brief: a.detail || a.brief || '',
  };
}

/** Parse text to format @mentions and [done] tags */
function renderBoardText(text: string) {
  const regex = /(@[a-zA-Z0-9_\-]+|\[(?:done|brief|handoff|claim)\])/g;
  const parts: { text: string; isMention?: boolean; isTag?: boolean }[] = [];
  let lastIndex = 0;

  let match: RegExpExecArray | null;
  while ((match = regex.exec(text)) !== null) {
    if (match.index > lastIndex) {
      parts.push({ text: text.slice(lastIndex, match.index) });
    }
    const token = match[1];
    if (token.startsWith('@')) {
      parts.push({ text: token, isMention: true });
    } else {
      parts.push({ text: token, isTag: true });
    }
    lastIndex = regex.lastIndex;
  }

  if (lastIndex < text.length) {
    parts.push({ text: text.slice(lastIndex) });
  }

  return (
    <>
      {parts.map((p, i) => {
        if (p.isMention) {
          return (
            <span
              key={i}
              className="inline-block px-1.5 py-0.2 rounded bg-[#F1C86A]/20 text-[#F1C86A] font-semibold font-mono text-[11px] mx-0.5 border border-[#F1C86A]/30"
            >
              {p.text}
            </span>
          );
        }
        if (p.isTag) {
          return (
            <span
              key={i}
              className="inline-block px-1.5 py-0.2 rounded bg-emerald-500/20 text-emerald-300 font-bold font-mono text-[10px] uppercase mx-0.5 border border-emerald-500/30"
            >
              {p.text}
            </span>
          );
        }
        return <span key={i}>{p.text}</span>;
      })}
    </>
  );
}

export function SwarmVisualizer({
  className,
  title = 'AlphaCode Multi-Agent Swarm',
  workflow,
  agents: propAgents,
  messages: propMessages,
  onPostMessage,
  onLaunchSwarm,
}: SwarmVisualizerProps) {
  const { dir, lang } = useTranslation();
  const isAr = lang === 'ar';

  const [activeFilterAgent, setActiveFilterAgent] = useState<string | null>(null);
  const [boardInput, setBoardInput] = useState('');
  const [swarmTaskInput, setSwarmTaskInput] = useState('');

  const agents: SwarmAgent[] = useMemo(() => {
    if (propAgents && propAgents.length > 0) return propAgents;
    if (workflow?.agents && workflow.agents.length > 0) {
      return workflow.agents.map(mapAgentStatusToSwarmAgent);
    }
    return [];
  }, [propAgents, workflow?.agents]);

  const messages: SwarmBoardMessage[] = useMemo(() => {
    if (propMessages && propMessages.length > 0) return propMessages;
    if (workflow?.swarmMessages && workflow.swarmMessages.length > 0) {
      return workflow.swarmMessages;
    }
    return [];
  }, [propMessages, workflow?.swarmMessages]);

  const todos: TodoItem[] = workflow?.todos ?? [];
  const planText = workflow?.plan ?? '';

  const filteredMessages = useMemo(() => {
    if (!activeFilterAgent) return messages;
    return messages.filter(
      m => m.agent === activeFilterAgent || m.text.includes(`@${activeFilterAgent}`),
    );
  }, [messages, activeFilterAgent]);

  const handleSend = () => {
    const trimmed = boardInput.trim();
    if (!trimmed) return;
    onPostMessage?.(trimmed);
    setBoardInput('');
  };

  const handleStartSwarm = () => {
    const trimmed = swarmTaskInput.trim();
    if (!trimmed) return;
    onLaunchSwarm?.(trimmed);
    setSwarmTaskInput('');
  };

  const getAgentStateBadge = (state: AgentState) => {
    switch (state) {
      case 'working':
        return (
          <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[11px] font-semibold bg-emerald-500/15 text-emerald-300 border border-emerald-500/30 shadow-sm">
            <Loader2 className="w-3 h-3 text-emerald-400 animate-spin" />
            {isAr ? 'يعمل' : 'Working'}
          </span>
        );
      case 'waiting':
        return (
          <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[11px] font-semibold bg-amber-500/15 text-amber-300 border border-amber-500/30">
            <Clock className="w-3 h-3 text-amber-400" />
            {isAr ? 'في الانتظار' : 'Waiting'}
          </span>
        );
      case 'done':
        return (
          <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[11px] font-semibold bg-sky-500/15 text-sky-300 border border-sky-500/30">
            <CheckCircle2 className="w-3 h-3 text-sky-400" />
            {isAr ? 'مكتمل' : 'Done'}
          </span>
        );
      case 'killed':
        return (
          <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[11px] font-semibold bg-zinc-800 text-zinc-400 border border-white/5">
            <Skull className="w-3 h-3 text-zinc-500" />
            {isAr ? 'متوقف' : 'Stopped'}
          </span>
        );
    }
  };

  const hasAnySwarmState = agents.length > 0 || messages.length > 0 || todos.length > 0 || Boolean(planText);

  return (
    <div
      className={cn(
        'flex flex-col bg-[#0A0D14] border border-[var(--ac-border)] rounded-xl overflow-hidden shadow-2xl',
        className,
      )}
      dir={dir}
    >
      {/* Top Banner */}
      <div className="flex flex-wrap items-center justify-between gap-3 p-4 bg-[#121620] border-b border-[var(--ac-border)] shrink-0">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-xl bg-gradient-to-br from-[#F1C86A]/20 to-[#393124] border border-[#F1C86A]/30 text-[#F1C86A]">
            <Users className="w-5 h-5" />
          </div>
          <div>
            <h2 className="text-base font-bold text-white tracking-tight flex items-center gap-2">
              <span>{isAr ? 'منسق سرب الوكلاء المتعددين' : title}</span>
              <span
                className={cn(
                  'text-[10px] font-mono font-semibold uppercase px-2 py-0.5 rounded border',
                  agents.length > 0
                    ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20'
                    : 'bg-white/5 text-zinc-400 border-white/10',
                )}
              >
                {agents.length > 0 ? (isAr ? 'نشط' : 'Live') : (isAr ? 'خامل' : 'Idle')}
              </span>
            </h2>
            <p className="text-xs text-zinc-400">
              {isAr
                ? `متصل بحالة سير العمل الحية • ${agents.length} وكيل • ${todos.length} مهمة`
                : `Live backend workflow state • ${agents.length} agents • ${todos.length} todos`}
            </p>
          </div>
        </div>

        {/* Swarm Quick Stats */}
        <div className="flex items-center gap-2 text-xs font-mono">
          <span className="px-2.5 py-1 rounded-lg bg-[#181D29] border border-white/10 text-zinc-300 flex items-center gap-1.5">
            <Cpu className="w-3.5 h-3.5 text-[#F1C86A]" />
            <span>{isAr ? 'النشطون:' : 'Active:'} {agents.filter(a => a.state === 'working').length}</span>
          </span>
          <span className="px-2.5 py-1 rounded-lg bg-[#181D29] border border-white/10 text-zinc-300 flex items-center gap-1.5">
            <MessageSquare className="w-3.5 h-3.5 text-sky-400" />
            <span>{messages.length} {isAr ? 'رسائل' : 'msgs'}</span>
          </span>
        </div>
      </div>

      {!hasAnySwarmState ? (
        /* Clean Empty State when no swarm is active */
        <div className="flex-1 flex flex-col items-center justify-center p-8 text-center">
          <div className="w-16 h-16 rounded-2xl bg-[#F1C86A]/10 border border-[#F1C86A]/25 flex items-center justify-center mb-4">
            <Users className="w-8 h-8 text-[#F1C86A]" />
          </div>
          <h3 className="text-base font-bold text-white mb-1">
            {isAr ? 'لا يوجد سرب وكلاء نشط حالياً' : 'No Active Swarm Session'}
          </h3>
          <p className="text-xs text-zinc-400 max-w-md mb-5 leading-relaxed">
            {isAr
              ? 'ابدأ تنفيذ `/swarm` في المحادثة أو أدخل مهمة أدناه لإطلاق وكلاء متعاونين على مساحة العمل النشطة.'
              : 'Launch a multi-agent `/swarm` workflow from here or via chat to inspect live agent states, plans, todos, and board messages.'}
          </p>

          <div className="flex items-center gap-2 w-full max-w-lg">
            <input
              type="text"
              value={swarmTaskInput}
              onChange={e => setSwarmTaskInput(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleStartSwarm()}
              placeholder={isAr ? 'مثال: راجع الأمان وأصلح الاختبارات عبر السرب…' : 'e.g. Audit security and fix failing tests across modules…'}
              className="flex-1 bg-[#181D29] border border-white/10 rounded-xl px-3.5 py-2 text-xs text-white placeholder-zinc-500 focus:outline-none focus:border-[#F1C86A]"
            />
            <button
              onClick={handleStartSwarm}
              disabled={!swarmTaskInput.trim()}
              className="flex items-center gap-1.5 px-4 py-2 rounded-xl text-xs font-bold bg-[#F1C86A] hover:bg-[#FFD34D] text-[#211909] shadow-md disabled:opacity-40 cursor-pointer transition-all"
            >
              <Play className="w-3.5 h-3.5 fill-current" />
              <span>{isAr ? 'بدء /swarm' : 'Start /swarm'}</span>
            </button>
          </div>
        </div>
      ) : (
        <div className="flex-1 flex flex-col overflow-hidden">
          {/* Roster Cards Grid */}
          {agents.length > 0 && (
            <div className="p-4 bg-[#0E121A] border-b border-[var(--ac-border)] shrink-0">
              <div className="flex items-center justify-between mb-2.5">
                <h3 className="text-xs font-bold text-zinc-300 uppercase tracking-wider flex items-center gap-1.5">
                  <Bot className="w-3.5 h-3.5 text-[#F1C86A]" />
                  {isAr ? 'الوكلاء النشطون' : 'Active Swarm Roster'}
                </h3>
                {activeFilterAgent && (
                  <button
                    onClick={() => setActiveFilterAgent(null)}
                    className="text-[11px] text-[#F1C86A] hover:underline"
                  >
                    {isAr ? `إلغاء التصفية (@${activeFilterAgent})` : `Clear filter (@${activeFilterAgent})`}
                  </button>
                )}
              </div>

              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-3">
                {agents.map(agent => {
                  const isSelected = activeFilterAgent === agent.name;
                  return (
                    <div
                      key={agent.id}
                      onClick={() => setActiveFilterAgent(isSelected ? null : agent.name)}
                      className={cn(
                        'p-3 rounded-xl border transition-all cursor-pointer flex flex-col justify-between group relative overflow-hidden',
                        isSelected
                          ? 'bg-[#1A202F] border-[#F1C86A] shadow-lg shadow-[#F1C86A]/10'
                          : 'bg-[#121622] border-white/5 hover:border-white/15 hover:bg-[#161C2A]',
                      )}
                    >
                      <div>
                        <div className="flex items-center justify-between gap-1.5 mb-2">
                          <div className="flex items-center gap-2 min-w-0">
                            <div
                              className={cn(
                                'w-6 h-6 rounded-md bg-gradient-to-tr flex items-center justify-center font-bold text-xs text-white shadow-sm shrink-0',
                                agent.avatarColor,
                              )}
                            >
                              {agent.name.charAt(0).toUpperCase()}
                            </div>
                            <span className="text-xs font-mono font-bold text-white truncate">
                              @{agent.name}
                            </span>
                          </div>
                          {getAgentStateBadge(agent.state)}
                        </div>

                        <div className="text-[11px] text-zinc-400 font-medium mb-1 line-clamp-1">
                          {agent.role}
                        </div>
                        {agent.brief && (
                          <p className="text-[10px] text-zinc-500 leading-normal line-clamp-2 mb-2 font-mono">
                            {agent.brief}
                          </p>
                        )}
                      </div>

                      <div className="flex items-center justify-between text-[10px] font-mono text-zinc-400 pt-2 border-t border-white/5">
                        <span className="text-zinc-500 truncate max-w-[140px]">
                          {agent.model || 'session'}
                        </span>
                        {agent.tasksCompleted !== undefined && agent.totalTasks !== undefined && (
                          <span className="text-emerald-400 font-semibold">
                            {agent.tasksCompleted}/{agent.totalTasks}
                          </span>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {/* Plan & Todos strip when present */}
          {(planText || todos.length > 0) && (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3 p-4 bg-[#0C1018] border-b border-[var(--ac-border)] shrink-0 max-h-48 overflow-y-auto">
              {planText && (
                <div className="p-3 rounded-xl bg-[#121622] border border-white/5 space-y-1.5">
                  <div className="flex items-center gap-1.5 text-xs font-bold text-[#F1C86A]">
                    <Compass className="w-3.5 h-3.5" />
                    <span>{isAr ? 'خطة التنفيذ الحالية' : 'Active Workflow Plan'}</span>
                  </div>
                  <pre className="text-[11px] text-zinc-300 whitespace-pre-wrap font-mono leading-relaxed">
                    {planText}
                  </pre>
                </div>
              )}
              {todos.length > 0 && (
                <div className="p-3 rounded-xl bg-[#121622] border border-white/5 space-y-1.5">
                  <div className="flex items-center justify-between text-xs font-bold text-emerald-300">
                    <span className="flex items-center gap-1.5">
                      <ListChecks className="w-3.5 h-3.5" />
                      {isAr ? 'المهام' : 'Workflow Todos'}
                    </span>
                    <span className="font-mono text-[11px] text-zinc-400">
                      {todos.filter(t => t.done).length}/{todos.length}
                    </span>
                  </div>
                  <div className="space-y-1">
                    {todos.map(t => (
                      <div key={t.id} className="flex items-center gap-2 text-[11px]">
                        {t.done ? (
                          <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                        ) : (
                          <AlertCircle className="w-3.5 h-3.5 text-amber-400 shrink-0" />
                        )}
                        <span className={cn(t.done ? 'line-through text-zinc-500' : 'text-zinc-200')}>
                          {t.title}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}

          {/* Live Swarm Board Message Stream */}
          <div className="flex-1 flex flex-col min-h-[200px] bg-[#0A0D14] overflow-hidden">
            <div className="flex items-center justify-between px-4 py-2 bg-[#121620] border-b border-white/5 text-xs text-zinc-400 shrink-0">
              <div className="flex items-center gap-2">
                <Hash className="w-3.5 h-3.5 text-[#F1C86A]" />
                <span className="font-semibold text-zinc-200">
                  {isAr ? 'سجل رسائل السرب' : 'Swarm Activity & Messages'}
                </span>
                {activeFilterAgent && (
                  <span className="text-zinc-400 font-mono text-[11px]">
                    (filtered: @{activeFilterAgent})
                  </span>
                )}
              </div>
              <span className="text-[11px] font-mono text-zinc-500">
                {filteredMessages.length} {isAr ? 'رسالة' : 'messages'}
              </span>
            </div>

            <div className="flex-1 overflow-y-auto p-4 space-y-3 font-mono text-xs">
              {filteredMessages.length === 0 ? (
                <div className="text-center py-8 text-zinc-500 text-xs font-sans">
                  {isAr
                    ? 'لا توجد رسائل مسجلة بعد. استخدم الحقل أدناه لإرسال توجيه للسرب.'
                    : 'No swarm board messages recorded yet. Post an instruction below to dispatch to the active session.'}
                </div>
              ) : (
                filteredMessages.map(msg => {
                  const author = agents.find(a => a.name === msg.agent);
                  const avatarGrad = author?.avatarColor || 'from-zinc-500 to-zinc-700';
                  return (
                    <div
                      key={msg.seq}
                      className="flex items-start gap-3 p-3 rounded-xl bg-[#121622] border border-white/5 hover:border-white/10 transition-colors"
                    >
                      <div
                        className={cn(
                          'w-7 h-7 rounded-lg bg-gradient-to-tr flex items-center justify-center font-bold text-xs text-white shadow-sm shrink-0 mt-0.5',
                          avatarGrad,
                        )}
                      >
                        {msg.agent.charAt(0).toUpperCase()}
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center justify-between gap-2 mb-1">
                          <div className="flex items-center gap-2">
                            <span className="font-bold text-white font-mono text-xs">@{msg.agent}</span>
                            <span className="text-[10px] text-zinc-500">
                              {author?.role || 'Agent'}
                            </span>
                          </div>
                          <div className="flex items-center gap-2 text-[10px] text-zinc-500">
                            <span>{formatTime(msg.timestamp)}</span>
                            <span className="px-1.5 py-0.2 rounded bg-white/5 border border-white/5 text-zinc-400">
                              #{msg.seq}
                            </span>
                          </div>
                        </div>
                        <div className="text-xs text-zinc-300 leading-relaxed break-words font-sans whitespace-pre-wrap">
                          {renderBoardText(msg.text)}
                        </div>
                      </div>
                    </div>
                  );
                })
              )}
            </div>

            {/* Post Message Input Bar */}
            <div className="p-3 bg-[#0E121A] border-t border-[var(--ac-border)] flex items-center gap-2 shrink-0">
              <input
                type="text"
                value={boardInput}
                onChange={e => setBoardInput(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && handleSend()}
                placeholder={
                  isAr
                    ? "أرسل توجيهاً أو أمراً للجلسة/السرب (مثال: '/swarm status' أو '@agent ...')"
                    : "Send instruction or command to active swarm/session (e.g. '/swarm status')"
                }
                className="flex-1 bg-[#181D29] border border-white/10 rounded-lg px-3 py-1.5 text-xs text-white placeholder-zinc-500 focus:outline-none focus:border-[#F1C86A]"
              />
              <button
                onClick={handleSend}
                disabled={!boardInput.trim()}
                className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg text-xs font-semibold bg-[#F1C86A] hover:bg-[#FFD34D] text-[#211909] shadow-md shadow-[#F1C86A]/20 active:scale-95 transition-all disabled:opacity-40 cursor-pointer"
              >
                <Send className="w-3.5 h-3.5" />
                <span>{isAr ? 'إرسال' : 'Send'}</span>
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
