//! Abas do navegador separadas por projeto. Cada projeto tem o seu conjunto de abas, com limite próprio,
//! e a aba ativa de cada um. Só o projeto atual aparece no painel. Estado puro: as webviews ficam em `browser.rs`.
use std::collections::BTreeMap;

/// Limite de abas por projeto: cada uma é um processo do WebView2, então é preciso pôr um teto.
pub const MAX_TABS: usize = 8;

/// Uma aba aberta. `economize` pede suspensão quando a aba fica escondida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub id: u32,
    pub project: String,
    pub economize: bool,
}

#[derive(Debug, Default)]
pub struct ProjectTabs {
    next: u32,
    tabs: Vec<Tab>,
    active: BTreeMap<String, u32>,
    current: Option<String>,
}

impl ProjectTabs {
    pub const fn new() -> Self {
        Self { next: 0, tabs: Vec::new(), active: BTreeMap::new(), current: None }
    }

    /// Projeto mostrado no painel. Sem projeto escolhido, nenhuma aba aparece.
    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    pub fn set_current(&mut self, project: &str) {
        self.current = Some(project.to_string());
    }

    /// Reserva o id de uma aba nova no projeto, ou recusa quando o projeto já tem o limite de abas.
    pub fn reserve(&mut self, project: &str) -> Result<u32, String> {
        if self.tabs_in(project).count() >= MAX_TABS {
            return Err(format!("o navegador aceita até {MAX_TABS} abas neste projeto; feche uma para abrir outra"));
        }
        self.next += 1;
        self.tabs.push(Tab { id: self.next, project: project.to_string(), economize: false });
        Ok(self.next)
    }

    /// Todas as abas abertas, de todos os projetos.
    pub fn all(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn project_of(&self, id: u32) -> Option<&str> {
        self.tabs.iter().find(|tab| tab.id == id).map(|tab| tab.project.as_str())
    }

    /// Abas do projeto, na ordem em que foram abertas.
    pub fn tabs_in<'a>(&'a self, project: &'a str) -> impl Iterator<Item = &'a Tab> + 'a {
        self.tabs.iter().filter(move |tab| tab.project == project)
    }

    pub fn active_in(&self, project: &str) -> Option<u32> {
        self.active.get(project).copied().filter(|id| self.project_of(*id) == Some(project))
    }

    /// Aba ativa do projeto atual, a única que fica visível no painel.
    pub fn active_of_current(&self) -> Option<u32> {
        self.current().and_then(|project| self.active_in(project))
    }

    /// Torna a aba ativa do próprio projeto. Devolve false se ela não existe.
    pub fn set_active(&mut self, id: u32) -> bool {
        let Some(project) = self.project_of(id).map(str::to_string) else {
            return false;
        };
        self.active.insert(project, id);
        true
    }

    /// Tira a aba da lista. Se era a ativa do projeto, a última aberta dele passa a ser a ativa.
    /// Devolve a ativa do projeto depois do fechamento, se houver.
    pub fn forget(&mut self, id: u32) -> Option<u32> {
        let project = self.project_of(id)?.to_string();
        self.tabs.retain(|tab| tab.id != id);
        if self.active.get(&project) == Some(&id) {
            match self.tabs_in(&project).last().map(|tab| tab.id) {
                Some(last) => self.active.insert(project.clone(), last),
                None => self.active.remove(&project),
            };
        }
        self.active_in(&project)
    }

    /// Liga ou desliga a economia de memória de uma aba aberta. Devolve false se ela não existe.
    pub fn set_economy(&mut self, id: u32, on: bool) -> bool {
        match self.tabs.iter_mut().find(|tab| tab.id == id) {
            Some(tab) => {
                tab.economize = on;
                true
            }
            None => false,
        }
    }

    pub fn is_economy(&self, id: u32) -> bool {
        self.tabs.iter().any(|tab| tab.id == id && tab.economize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserves_up_to_the_limit_per_project_then_refuses() {
        let mut tabs = ProjectTabs::new();
        for _ in 0..MAX_TABS {
            tabs.reserve("/a").unwrap();
        }

        let err = tabs.reserve("/a").unwrap_err();
        assert!(err.contains(&format!("{MAX_TABS} abas")), "{err}");
    }

    #[test]
    fn the_limit_of_one_project_does_not_count_the_others() {
        let mut tabs = ProjectTabs::new();
        for _ in 0..MAX_TABS {
            tabs.reserve("/a").unwrap();
        }

        assert!(tabs.reserve("/b").is_ok());
    }

    #[test]
    fn each_project_only_lists_its_own_tabs() {
        let mut tabs = ProjectTabs::new();
        let a = tabs.reserve("/a").unwrap();
        let b = tabs.reserve("/b").unwrap();

        let in_a: Vec<u32> = tabs.tabs_in("/a").map(|tab| tab.id).collect();
        assert_eq!(in_a, vec![a]);
        assert_eq!(tabs.project_of(b), Some("/b"));
    }

    #[test]
    fn switching_projects_keeps_each_active_tab() {
        let mut tabs = ProjectTabs::new();
        let a1 = tabs.reserve("/a").unwrap();
        let a2 = tabs.reserve("/a").unwrap();
        let b1 = tabs.reserve("/b").unwrap();
        tabs.set_current("/a");
        assert!(tabs.set_active(a1));
        tabs.set_current("/b");
        assert!(tabs.set_active(b1));

        tabs.set_current("/a");
        assert_eq!(tabs.active_of_current(), Some(a1));
        assert_ne!(tabs.active_of_current(), Some(a2));
        tabs.set_current("/b");
        assert_eq!(tabs.active_of_current(), Some(b1));
    }

    #[test]
    fn closing_the_active_tab_hands_over_within_its_project_only() {
        let mut tabs = ProjectTabs::new();
        let a1 = tabs.reserve("/a").unwrap();
        let a2 = tabs.reserve("/a").unwrap();
        let b1 = tabs.reserve("/b").unwrap();
        tabs.set_active(a1);
        tabs.set_active(b1);

        assert_eq!(tabs.forget(a1), Some(a2));
        assert_eq!(tabs.active_in("/b"), Some(b1));
    }

    #[test]
    fn closing_the_last_tab_of_a_project_leaves_it_without_active_tab() {
        let mut tabs = ProjectTabs::new();
        let only = tabs.reserve("/a").unwrap();
        tabs.set_active(only);

        assert_eq!(tabs.forget(only), None);
        assert_eq!(tabs.active_in("/a"), None);
        assert!(tabs.tabs_in("/a").next().is_none());
    }

    #[test]
    fn economy_is_set_per_tab_and_leaves_with_the_tab() {
        let mut tabs = ProjectTabs::new();
        let first = tabs.reserve("/a").unwrap();
        let second = tabs.reserve("/a").unwrap();
        assert!(tabs.set_economy(first, true));
        assert!(tabs.is_economy(first));
        assert!(!tabs.is_economy(second));
        tabs.forget(first);
        assert!(!tabs.is_economy(first));
        assert!(!tabs.set_economy(first, true), "aba fechada não recebe economia");
    }

    #[test]
    fn a_tab_of_another_project_is_not_active_in_the_current_one() {
        let mut tabs = ProjectTabs::new();
        tabs.reserve("/a").unwrap();
        let b = tabs.reserve("/b").unwrap();
        tabs.set_current("/a");

        assert!(tabs.set_active(b));
        assert_eq!(tabs.active_of_current(), None, "a aba de /b não aparece em /a");
    }
}
