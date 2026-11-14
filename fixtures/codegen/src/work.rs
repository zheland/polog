#[cfg(feature = "more-args")]
fn gen_trace_id_and_data() -> ([u8; 8], [u8; 8]) {
    use rand::distr::Alphanumeric;
    use rand::RngExt;

    let mut rng = rand::rng();
    let mut trace_id = [0u8; 8];
    for byte in &mut trace_id {
        *byte = rng.sample(Alphanumeric);
    }

    let trace_data: [u8; 8] = rand::rng().random();

    (trace_id, trace_data)
}

macro_rules! m {
    ( $name:ident = $d:tt $pmin:literal : $pmax:literal $(+ $num_children:literal * $child:ident)* ) => {
        pub mod $name {
            #![allow(unused_imports, unused_mut, unused_variables, reason = "codegen")]

            use core::hint::black_box;

            use crate::common::{DetachedLogger, Span};
            use crate::{into_detached, log, nested_span};
            use crate::work::gen_trace_id_and_data;

            use rand::RngExt;
            use rand::distr::{Alphanumeric, SampleString};

            #[inline]
            pub fn compute_cost_sync(span: &mut Span<'_>) -> u64 {
                let price = rand::random_range($pmin..=$pmax);
                log!(span, stringify!($name), price);

                #[cfg(feature = "more-args")]
                {
                    let (trace_id, trace_data) = gen_trace_id_and_data();
                    let trace_id = core::str::from_utf8(&trace_id).unwrap();
                    let trace_data = trace_data.as_slice();

                    #[cfg(feature = "log")]
                    ::log::info!(trace_id, trace_data:?; "Trace ID");
                    #[cfg(feature = "tracing")]
                    ::tracing::info!(trace_id, trace_data, "Trace ID");
                    #[cfg(feature = "slog")]
                    ::slog::info!(
                        &span.slog, "Trace ID";
                        "trace_id" => trace_id, "trace_data" => trace_data
                    );
                    #[cfg(feature = "polog")]
                    ::polog::info!(
                        &mut span.polog, "Trace ID"; :t trace_id, :t ? trace_data
                    );
                }

                let mut cost = price;
                $(
                    for i in 0..$num_children {
                        #[cfg(feature = "more-args")]
                        let mut span = {
                            let (trace_id, trace_data) = gen_trace_id_and_data();
                            let trace_id = core::str::from_utf8(&trace_id).unwrap();
                            let trace_data = trace_data.as_slice();

                            #[cfg(all(feature = "span", feature = "tracing"))]
                            let tracing = ::tracing::info_span!(
                                stringify!($child), i, trace_id, trace_data
                            ).entered();

                            #[cfg(all(feature = "context", feature = "slog"))]
                            let slog = span.slog.new(
                                ::slog::o!(
                                    "msg" => stringify!($child),
                                    "i" => i,
                                    "trace_id" => trace_id.to_owned(),
                                    "trace_data" => trace_data.to_owned(),
                                )
                            );
                            #[cfg(all(not(feature = "context"), feature = "slog"))]
                            let slog = span.slog.clone();

                            #[cfg(all(feature = "span", feature = "polog"))]
                            let polog = ::polog::info_span!(
                                &mut span.polog,
                                stringify!($child);
                                i, :t trace_id, :t ? trace_data
                            );
                            #[cfg(all(not(feature = "span"), feature = "polog"))]
                            let polog = ::polog::LoggerExt::transparent_borrowing_span(&mut span.polog);

                            $crate::common::Span {
                                #[cfg(all(feature = "span", feature = "tracing"))]
                                _tracing: tracing,
                                #[cfg(feature = "slog")]
                                slog,
                                #[cfg(feature = "polog")]
                                polog,
                                #[cfg(not(feature = "polog"))]
                                _lt: core::marker::PhantomData,
                            }
                        };

                        #[cfg(not(feature = "more-args"))]
                        let mut span = nested_span!(span, stringify!($child), i);


                        cost += super::$child::compute_cost_sync(&mut span);
                    }
                )*

                #[cfg(feature = "more-args")]
                {
                    let data: ([u8; 8], char, f32, bool) = rand::rng().random();
                    let data_ref = &data;

                    #[cfg(feature = "log")]
                    ::log::info!(data:?, data_ref:?; "Debug data" );
                    #[cfg(feature = "tracing")]
                    ::tracing::info!(? data, ? data_ref, "Debug data" );
                    #[cfg(feature = "slog")]
                    ::slog::info!(
                        &span.slog, "Debug data"; "data" => ? data, "data_ref" => ? data_ref );
                    #[cfg(feature = "polog")]
                    ::polog::info!(
                        &mut span.polog, "Debug data"; ? data, :t ? data_ref );
                }

                log!(span, stringify!($name), price, cost);

                cost
            }

            #[inline]
            pub async fn compute_cost_async(mut span: DetachedLogger) -> u64 {
                let mut set = tokio::task::JoinSet::<u64>::new();

                let price = rand::random_range($pmin..=$pmax);
                log!(span, stringify!($name), price);

                $(
                    for i in 0..$num_children {
                        let span = nested_span!(span, stringify!($child), i);
                        let span = into_detached!(span);
                        let _handle = set.spawn(super::$child::compute_cost_async(span));
                    }
                )*

                let mut cost = price;
                while let Some(task) = set.join_next().await {
                    cost += task.unwrap();
                }

                log!(span, stringify!($name), price, cost);

                cost
            }
        }
    };
}

macro_rules! cfg_block {
    ($($item:item)*) => {
        $($item)*
    };
}

// Data center components and prices are mostly LLM-generated.

m!(datacenters = $0:0 + 1 * region);

#[cfg(not(feature = "complex"))]
cfg_block! {
    m!(region = $30_000_000:35_000_000);
}

#[cfg(feature = "complex")]
cfg_block! {
    m!(region = $0:0 + 2 * availability_zone);
    m!(availability_zone = $0:0 + 3 * datacenter);
    m!(datacenter = $0:0 + 4 * hall);
    m!(
        hall = $0:0 + 1 * cooling_system + 1 * power_distribution + 5 * rack
        + 1 * fire_suppression + 1 * security_system + 1 * monitoring
    );

    m!(cooling_system = $0:0 + 2 * chiller + 4 * ahu + 10 * pump + 20 * fan + 50 * pipe);
    m!(power_distribution = $0:0 + 2 * transformer + 4 * switchgear + 8 * ups + 20 * pdu);
    m!(rack = $0:0 + 1 * frame + 2 * pdu + 1 * switch + 1 * kvm + 10 * server + 1 * cable_manager);

    m!(
        server = $0:0 + 2 * cpu + 4 * ram + 2 * gpu + 4 * disk + 4 * fan + 2 * nic
        + 1 * motherboard + 2 * power_supply + 1 * chassis + 1 * backplane
    );

    m!(
        cpu = $0:0 + 8 * cpu_core + 1 * memory_controller + 1 * pcie_controller
        + 1 * l3_cache + 1 * io_controller
    );
    m!(
        cpu_core = $0:0 + 2 * alu + 1 * fpu + 1 * l1_cache + 1 * l2_cache
        + 1 * decoder + 1 * scheduler + 1 * register_file
    );
    m!(alu = $0:0 + 1 * adder + 1 * multiplier + 1 * shifter + 1 * logic_unit);
    m!(fpu = $0:0 + 1 * fma + 1 * divider + 1 * sqrt);
    m!(l1_cache = $0:0 + 4 * cache_line);
    m!(l2_cache = $0:0 + 8 * cache_line);
    m!(l3_cache = $0:0 + 16 * cache_line);
    m!(cache_line = $0:0 + 2 * transistor + 1 * tag + 1 * valid_bit);
    m!(transistor = $0:0 + 1 * gate + 1 * source + 1 * drain);
    m!(memory_controller = $0:0 + 2 * channel);
    m!(channel = $0:0 + 2 * bank);
    m!(bank = $0:0 + 4 * row);
    m!(row = $0:0 + 4 * column);
    m!(column = $0:0 + 1 * capacitor + 1 * transistor);
    m!(pcie_controller = $0:0 + 16 * lane);
    m!(lane = $0:0 + 1 * lane_transmitter + 1 * receiver);
    m!(io_controller = $0:0 + 1 * sata_controller + 1 * usb_controller + 1 * ethernet_controller);
    m!(register_file = $0:0 + 32 * register);

    m!(ram = $0:0 + 4 * chip + 1 * pcb + 1 * heat_spreader);
    m!(chip = $0:0 + 2 * bank + 1 * controller);

    m!(gpu = $0:0 + 16 * shader_core + 2 * memory_controller + 1 * video_output + 1 * cooler);
    m!(shader_core = $0:0 + 4 * alu + 2 * fpu + 1 * register_file + 1 * scheduler);
    m!(video_output = $0:0 + 1 * hdmi + 1 * displayport);
    m!(cooler = $0:0 + 2 * fan + 1 * heatsink);

    m!(
        disk = $0:0 + 2 * platter + 2 * disk_head + 1 * spindle_motor
        + 1 * actuator + 1 * controller + 1 * cache
    );
    m!(platter = $0:0 + 1 * surface);
    m!(surface = $0:0 + 2 * track);
    m!(track = $0:0 + 2 * sector);
    m!(sector = $1:1);
    m!(disk_head = $10:50);
    m!(spindle_motor = $20:100);
    m!(actuator = $50:200);
    m!(cache = $5:20);

    m!(fan = $0:0 + 1 * motor + 1 * blade + 1 * bearing);
    m!(blade = $1:5);
    m!(bearing = $1:5);

    m!(nic = $0:0 + 2 * port + 1 * controller + 1 * phy);
    m!(port = $0:0 + 1 * transceiver + 1 * connector);
    m!(transceiver = $10:50);
    m!(connector = $1:5);
    m!(phy = $5:20);

    m!(
        motherboard = $0:0 + 1 * pcb + 1 * chipset + 2 * vrm + 1 * bios + 1 * cmos
        + 1 * cmos_battery + 1 * socket + 2 * ram_slot + 1 * pcie_slot
        + 1 * sata_connector + 1 * fan_header
    );
    m!(chipset = $0:0 + 1 * northbridge + 1 * southbridge);
    m!(northbridge = $0:0 + 1 * memory_controller + 1 * pcie_controller);
    m!(
        southbridge = $0:0 + 1 * sata_controller + 1 * usb_controller
        + 1 * ethernet_controller + 1 * audio_controller
    );
    m!(vrm = $0:0 + 2 * mosfet + 1 * inductor + 2 * capacitor);
    m!(mosfet = $1:5);
    m!(inductor = $1:5);
    m!(capacitor = $1:5);
    m!(bios = $5:20);
    m!(cmos = $1:5);
    m!(cmos_battery = $1:5);
    m!(socket = $10:50);
    m!(ram_slot = $2:10);
    m!(pcie_slot = $5:20);
    m!(sata_connector = $2:10);
    m!(fan_header = $1:5);
    m!(audio_controller = $5:20);

    m!(
        power_supply = $0:0 + 1 * transformer + 2 * rectifier + 2 * capacitor
        + 1 * fan + 1 * controller
    );
    m!(rectifier = $10:50);

    m!(chassis = $0:0 + 1 * frame + 2 * rail + 1 * front_panel + 1 * back_panel);
    m!(rail = $5:20);
    m!(front_panel = $10:50);
    m!(back_panel = $10:50);

    m!(backplane = $0:0 + 10 * connector + 1 * pcb);
    m!(pcb = $10:50);

    m!(chiller = $0:0 + 1 * compressor + 1 * condenser + 1 * evaporator + 1 * expansion_valve);
    m!(compressor = $500:2000);
    m!(condenser = $300:1500);
    m!(evaporator = $400:1800);
    m!(expansion_valve = $50:200);

    m!(ahu = $0:0 + 2 * filter + 1 * fan + 1 * cooling_coil + 1 * heater);
    m!(filter = $10:50);
    m!(cooling_coil = $100:500);
    m!(heater = $50:200);

    m!(pump = $0:0 + 1 * motor + 1 * impeller + 1 * seal);
    m!(motor = $20:100);
    m!(impeller = $10:50);
    m!(seal = $2:10);

    m!(pipe = $1:10);

    m!(transformer = $0:0 + 1 * magnetic_core + 2 * winding + 1 * tank + 1 * coolant);
    m!(magnetic_core = $100:500);
    m!(winding = $50:200);
    m!(tank = $20:100);
    m!(coolant = $5:20);

    m!(switchgear = $0:0 + 2 * breaker + 1 * busbar + 1 * enclosure);
    m!(breaker = $10:50);
    m!(busbar = $20:100);
    m!(enclosure = $50:200);

    m!(ups = $0:0 + 10 * battery + 1 * inverter + 1 * rectifier + 1 * controller);
    m!(battery = $0:0 + 2 * cell);
    m!(cell = $0:0 + 1 * electrode + 1 * separator + 1 * electrolyte);
    m!(electrode = $1:5);
    m!(separator = $1:2);
    m!(electrolyte = $1:3);
    m!(inverter = $200:1000);
    m!(controller = $50:200);

    m!(pdu = $0:0 + 4 * outlet + 1 * breaker + 1 * meter);
    m!(outlet = $2:10);
    m!(meter = $20:100);

    m!(frame = $100:500);

    m!(switch = $0:0 + 8 * port + 1 * asic + 1 * cpu + 1 * ram + 1 * flash);
    m!(asic = $0:0 + 1 * die + 1 * package);
    m!(die = $0:0 + 4 * transistor);
    m!(package = $0:0 + 1 * substrate + 1 * lid);
    m!(substrate = $5:20);
    m!(lid = $2:10);
    m!(flash = $10:50);

    m!(kvm = $50:200);
    m!(cable_manager = $20:100);

    m!(fire_suppression = $0:0 + 10 * sprinkler + 2 * tank + 10 * pipe + 1 * control_panel);
    m!(sprinkler = $0:0 + 1 * valve + 1 * sprinkler_head);
    m!(valve = $10:50);
    m!(sprinkler_head = $5:20);
    m!(control_panel = $200:1000);

    m!(security_system = $0:0 + 10 * camera + 4 * access_reader + 1 * control_panel);
    m!(camera = $0:0 + 1 * lens + 1 * camera_sensor + 1 * camera_housing);
    m!(lens = $20:100);
    m!(camera_sensor = $30:150);
    m!(camera_housing = $10:50);
    m!(access_reader = $0:0 + 1 * rfid + 1 * keypad + 1 * controller);
    m!(rfid = $10:50);
    m!(keypad = $5:20);

    m!(monitoring = $0:0 + 10 * sensor + 1 * server + 1 * software);
    m!(sensor = $0:0 + 1 * probe + 1 * sensor_transmitter);
    m!(probe = $5:20);
    m!(sensor_transmitter = $10:50);
    m!(software = $100:1000);

    m!(adder = $1:5);
    m!(multiplier = $2:10);
    m!(shifter = $1:5);
    m!(logic_unit = $1:5);
    m!(fma = $2:10);
    m!(divider = $2:10);
    m!(sqrt = $2:10);
    m!(tag = $1:2);
    m!(valid_bit = $1:1);
    m!(lane_transmitter = $5:20);
    m!(receiver = $5:20);
    m!(sata_controller = $10:50);
    m!(usb_controller = $5:20);
    m!(ethernet_controller = $10:50);
    m!(register = $1:2);
    m!(heat_spreader = $5:20);
    m!(hdmi = $5:20);
    m!(displayport = $5:20);
    m!(heatsink = $20:100);
    m!(gate = $1:2);
    m!(source = $1:2);
    m!(drain = $1:2);
    m!(decoder = $5:20);
    m!(scheduler = $5:20);
}
