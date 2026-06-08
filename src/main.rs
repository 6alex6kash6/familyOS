use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use encoding_rs::WINDOWS_1251;
use familyos::app::{AppService, CreateExpense, CreateGoal, ImportCsv, SettingsPatch};
use familyos::csv_import::CsvImportSource;
use familyos::render;

#[derive(Debug, Parser)]
#[command(name = "familyos")]
#[command(about = "Local-first household finance CLI and terminal UI")]
struct Cli {
    #[arg(
        long,
        env = "DATABASE_URL",
        default_value = "file:data/familyos.sqlite"
    )]
    database_url: String,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Status {
        #[arg(long)]
        json: bool,
    },
    Tui,
    Expenses {
        #[command(subcommand)]
        command: ExpenseCommand,
    },
    Goals {
        #[command(subcommand)]
        command: GoalCommand,
    },
    Taxes,
    Assets,
    Reports {
        #[command(subcommand)]
        command: Option<ReportCommand>,
    },
    Integrations {
        #[command(subcommand)]
        command: Option<IntegrationCommand>,
    },
    Settings {
        #[command(subcommand)]
        command: SettingsCommand,
    },
    Import {
        #[command(subcommand)]
        command: ImportCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ExpenseCommand {
    List,
    Add(AddExpenseArgs),
    Delete { id: String },
}

#[derive(Debug, Args)]
struct AddExpenseArgs {
    #[arg(long)]
    date: String,
    #[arg(long)]
    amount: f64,
    #[arg(long, default_value = "BYN")]
    currency: String,
    #[arg(long, default_value = "Прочее")]
    category: String,
    #[arg(long, default_value = "")]
    note: String,
    #[arg(long, default_value = "manual")]
    source: String,
}

#[derive(Debug, Subcommand)]
enum GoalCommand {
    List,
    Add(AddGoalArgs),
    Delete { id: String },
}

#[derive(Debug, Args)]
struct AddGoalArgs {
    #[arg(long)]
    name: String,
    #[arg(long)]
    target: f64,
    #[arg(long, default_value = "USDT")]
    currency: String,
    #[arg(long)]
    deadline: Option<String>,
    #[arg(long, default_value_t = 2)]
    priority: i64,
    #[arg(long, default_value_t = 0.0)]
    current: f64,
    #[arg(long)]
    monthly_required: Option<f64>,
    #[arg(long, default_value = "planned_purchase")]
    bucket: String,
}

#[derive(Debug, Subcommand)]
enum ReportCommand {
    DailyBrief,
    MonthlySnapshot,
    MonthlyClose,
}

#[derive(Debug, Subcommand)]
enum IntegrationCommand {
    Sync { provider: String },
    List,
}

#[derive(Debug, Subcommand)]
enum SettingsCommand {
    Set(SettingsArgs),
    Show,
}

#[derive(Debug, Args)]
struct SettingsArgs {
    #[arg(long)]
    monthly_income: Option<f64>,
    #[arg(long)]
    monthly_taxable_base: Option<f64>,
    #[arg(long)]
    tax_rate: Option<f64>,
    #[arg(long)]
    base_currency: Option<String>,
    #[arg(long)]
    life_currency: Option<String>,
}

#[derive(Debug, Subcommand)]
enum ImportCommand {
    Csv {
        #[arg(long)]
        source: String,
        #[arg(long)]
        file: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let service = AppService::connect_database_url(&cli.database_url)?;
    match cli.command.unwrap_or(Command::Status { json: false }) {
        Command::Status { json } => {
            let state = service.app_state()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&state)?);
            } else {
                print!("{}", render::render_overview(&state));
            }
        }
        Command::Tui => familyos::tui::run(&service)?,
        Command::Expenses { command } => match command {
            ExpenseCommand::List => print!("{}", render::render_expenses(&service.app_state()?)),
            ExpenseCommand::Add(args) => {
                service.create_expense(CreateExpense {
                    date: args.date,
                    amount: args.amount,
                    currency: args.currency,
                    category: args.category,
                    merchant_note: args.note,
                    source: args.source,
                })?;
                print!("{}", render::render_expenses(&service.app_state()?));
            }
            ExpenseCommand::Delete { id } => {
                service.delete_expense(&id)?;
                print!("{}", render::render_expenses(&service.app_state()?));
            }
        },
        Command::Goals { command } => match command {
            GoalCommand::List => print!("{}", render::render_goals(&service.app_state()?)),
            GoalCommand::Add(args) => {
                service.create_goal(CreateGoal {
                    name: args.name,
                    target_amount: args.target,
                    currency: args.currency,
                    deadline: args.deadline,
                    priority: args.priority,
                    current_amount: args.current,
                    monthly_required_amount: args.monthly_required,
                    bucket: args.bucket,
                })?;
                print!("{}", render::render_goals(&service.app_state()?));
            }
            GoalCommand::Delete { id } => {
                service.delete_goal(&id)?;
                print!("{}", render::render_goals(&service.app_state()?));
            }
        },
        Command::Taxes => print!("{}", render::render_taxes(&service.app_state()?)),
        Command::Assets => print!("{}", render::render_assets(&service.app_state()?)),
        Command::Reports { command } => match command.unwrap_or(ReportCommand::DailyBrief) {
            ReportCommand::DailyBrief => {
                service.generate_daily_brief()?;
                print!("{}", render::render_reports(&service.app_state()?));
            }
            ReportCommand::MonthlySnapshot => {
                service.create_monthly_snapshot()?;
                print!("{}", render::render_reports(&service.app_state()?));
            }
            ReportCommand::MonthlyClose => {
                service.generate_monthly_report()?;
                print!("{}", render::render_reports(&service.app_state()?));
            }
        },
        Command::Integrations { command } => match command.unwrap_or(IntegrationCommand::List) {
            IntegrationCommand::List => {
                print!("{}", render::render_integrations(&service.app_state()?))
            }
            IntegrationCommand::Sync { provider } => {
                service.sync_provider(&provider)?;
                print!("{}", render::render_integrations(&service.app_state()?));
            }
        },
        Command::Settings { command } => match command {
            SettingsCommand::Show => print!("{}", render::render_settings(&service.app_state()?)),
            SettingsCommand::Set(args) => {
                service.update_settings(SettingsPatch {
                    monthly_income: args.monthly_income,
                    monthly_taxable_base: args.monthly_taxable_base,
                    tax_rate: args.tax_rate,
                    base_currency: args.base_currency,
                    life_currency: args.life_currency,
                })?;
                print!("{}", render::render_settings(&service.app_state()?));
            }
        },
        Command::Import { command } => match command {
            ImportCommand::Csv { source, file } => {
                let source = CsvImportSource::parse(&source)?;
                let bytes = fs::read(&file)?;
                let csv = if source == CsvImportSource::Priorbank {
                    WINDOWS_1251.decode(&bytes).0.into_owned()
                } else {
                    String::from_utf8(bytes)?
                };
                service.import_csv(ImportCsv { source, csv })?;
                print!("{}", render::render_integrations(&service.app_state()?));
            }
        },
    }
    Ok(())
}
